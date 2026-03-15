use crate::error::{MoteError, Result};
use crate::state::AppState;
use axum::extract::State;
use axum::response::Json;
use serde_json::{json, Value};
use sqlx::Row;
use std::sync::Arc;
use uuid::Uuid;

// JSON-RPC 2.0 request structure
#[derive(serde::Deserialize)]
pub struct JsonRpcRequest {
    pub jsonrpc: String,
    pub method: String,
    pub params: Option<Value>,
    pub id: Option<Value>,
}

// JSON-RPC 2.0 response structure
#[derive(serde::Serialize)]
pub struct JsonRpcResponse {
    pub jsonrpc: String,
    pub result: Option<Value>,
    pub error: Option<JsonRpcError>,
    pub id: Option<Value>,
}

#[derive(serde::Serialize)]
pub struct JsonRpcError {
    code: i32,
    message: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    data: Option<Value>,
}

// Main MCP handler - handles raw JSON body
pub async fn handle_mcp(
    State(state): State<Arc<AppState>>,
    body: String,
) -> std::result::Result<Json<Value>, (axum::http::StatusCode, String)> {
    let request_id = Uuid::new_v4().to_string();
    
    // Parse JSON body
    let request_value: Value = serde_json::from_str(&body)
        .map_err(|_| (axum::http::StatusCode::BAD_REQUEST, json!({
            "jsonrpc": "2.0",
            "error": {
                "code": -32700,
                "message": "Parse error",
                "data": {
                    "request_id": request_id,
                    "timestamp": chrono::Utc::now().to_rfc3339()
                }
            },
            "id": null
        }).to_string()))?;

    // Check if this is a batch request (not supported)
    if request_value.is_array() {
        return Ok(Json(json!({
            "jsonrpc": "2.0",
            "error": {
                "code": -32700,
                "message": "Batch requests not supported",
                "data": {
                    "request_id": request_id,
                    "timestamp": chrono::Utc::now().to_rfc3339()
                }
            },
            "id": null
        })));
    }

    // Parse single request
    let request: JsonRpcRequest = serde_json::from_value(request_value)
        .map_err(|_| (axum::http::StatusCode::BAD_REQUEST, json!({
            "jsonrpc": "2.0",
            "error": {
                "code": -32600,
                "message": "Invalid Request",
                "data": {
                    "request_id": request_id,
                    "timestamp": chrono::Utc::now().to_rfc3339()
                }
            },
            "id": null
        }).to_string()))?;

    // Validate JSON-RPC version
    if request.jsonrpc != "2.0" {
        return Ok(Json(json!({
            "jsonrpc": "2.0",
            "error": {
                "code": -32600,
                "message": "Invalid JSON-RPC version",
                "data": {
                    "request_id": request_id,
                    "timestamp": chrono::Utc::now().to_rfc3339()
                }
            },
            "id": request.id
        })));
    }

    // Dispatch to method handler
    let result = match request.method.as_str() {
        "register_agent" => handle_register_agent(&state, request.params).await,
        "register_agent_simple" => handle_register_agent_simple(&state, request.params).await,
        "confirm_agent" => handle_confirm_agent(&state, request.params).await,
        "search_atoms" => handle_search_atoms(&state, request.params).await,
        "query_cluster" => handle_query_cluster(&state, request.params).await,
        "claim_direction" => handle_claim_direction(&state, request.params).await,
        "publish_atoms" => handle_publish_atoms(&state, request.params).await,
        "retract_atom" => handle_retract_atom(&state, request.params).await,
        "ban_atom" => handle_ban_atom(&state, request.params).await,
        "unban_atom" => handle_unban_atom(&state, request.params).await,
        "get_suggestions" => handle_get_suggestions(&state, request.params).await,
        "get_field_map" => handle_get_field_map(&state, request.params).await,
        "get_graph_edges" => handle_get_graph_edges(&state, request.params).await,
        _ => Err(MoteError::Validation(format!("Method not found: {}", request.method))),
    };

    // Format response
    match result {
        Ok(result_value) => Ok(Json(json!({
            "jsonrpc": "2.0",
            "result": result_value,
            "error": null,
            "id": request.id
        }))),
        Err(error) => Ok(Json(json!({
            "jsonrpc": "2.0",
            "result": null,
            "error": {
                "code": error.json_rpc_code(),
                "message": error.to_string(),
                "data": {
                    "request_id": request_id,
                    "timestamp": chrono::Utc::now().to_rfc3339(),
                }
            },
            "id": request.id
        }))),
    }
}

// Authentication and rate limiting for mutating methods.
// Accepts either token-based auth (api_token field) or Ed25519 signature-based auth
// (signature field). Token-based auth is the recommended path for AI agents.
pub async fn authenticate_and_rate_limit(
    state: &AppState,
    params: &Option<Value>,
) -> Result<String> {
    let params = params.as_ref().ok_or_else(|| MoteError::Validation("Missing params".to_string()))?;

    let agent_id: String = serde_json::from_value(params["agent_id"].clone())
        .map_err(|_| MoteError::Validation("agent_id field required".to_string()))?;

    // --- Token-based auth path (for AI agents, no crypto required) ---
    let api_token_val = &params["api_token"];
    if !api_token_val.is_null() {
        let api_token: String = serde_json::from_value(api_token_val.clone())
            .map_err(|_| MoteError::Validation("api_token must be a string".to_string()))?;

        let agent = crate::db::queries::get_agent_by_token(&state.pool, &api_token)
            .await?
            .ok_or_else(|| MoteError::Authentication("Invalid api_token".to_string()))?;

        if agent.agent_id != agent_id {
            return Err(MoteError::Authentication(
                "api_token does not match agent_id".to_string(),
            ));
        }

        if !state.rate_limiter.check_rate_limit(&agent_id, state.config.trust.max_atoms_per_hour) {
            state.metrics.rate_limit_rejections.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
            return Err(MoteError::RateLimit);
        }

        return Ok(agent_id);
    }

    // --- Ed25519 signature-based auth path (for cryptographic identity) ---
    let signature: String = serde_json::from_value(params["signature"].clone())
        .map_err(|_| MoteError::Validation("api_token or signature field required".to_string()))?;

    let agent = crate::db::queries::get_agent(&state.pool, &agent_id).await
        .map_err(|_| MoteError::Authentication("Agent not found".to_string()))?
        .ok_or_else(|| MoteError::Authentication("Agent not found".to_string()))?;

    if !agent.confirmed {
        return Err(MoteError::Authentication("Agent not confirmed".to_string()));
    }

    let params_without_signature = {
        let mut params_clone = params.clone();
        if let Some(obj) = params_clone.as_object_mut() {
            obj.remove("signature");
        }
        params_clone
    };

    let canonical_params = serde_json::to_string(&params_without_signature)
        .map_err(MoteError::Serialization)?;

    let signature_bytes = crate::crypto::signing::hex_to_bytes(&signature)?;
    let public_key_bytes = crate::crypto::signing::hex_to_bytes(&hex::encode(&agent.public_key))?;

    crate::crypto::signing::verify_signature(
        &public_key_bytes,
        canonical_params.as_bytes(),
        &signature_bytes,
    )?;

    if !state.rate_limiter.check_rate_limit(&agent_id, state.config.trust.max_atoms_per_hour) {
        state.metrics.rate_limit_rejections.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        return Err(MoteError::RateLimit);
    }

    Ok(agent_id)
}

// Handler implementations
pub async fn handle_register_agent(
    state: &AppState,
    params: Option<Value>,
) -> Result<Value> {
    let params = params.ok_or_else(|| MoteError::Validation("Missing params".to_string()))?;
    
    let public_key: String = serde_json::from_value(params["public_key"].clone())
        .map_err(|_| MoteError::Validation("public_key field required".to_string()))?;

    let registration = crate::domain::agent::AgentRegistration { public_key };
    
    let response = crate::db::queries::register_agent(&state.pool, registration).await?;
    
    Ok(json!({
        "agent_id": response.agent_id,
        "challenge": response.challenge
    }))
}

pub async fn handle_register_agent_simple(
    state: &AppState,
    params: Option<Value>,
) -> Result<Value> {
    let agent_name = params
        .as_ref()
        .and_then(|p| p.get("agent_name"))
        .and_then(|v| v.as_str())
        .unwrap_or("unnamed-agent")
        .to_string();

    let response = crate::db::queries::register_agent_simple(&state.pool).await?;

    Ok(json!({
        "agent_id": response.agent_id,
        "api_token": response.api_token,
        "agent_name": agent_name,
        "message": "Agent registered. Save agent_id and api_token — pass both to publish_atoms, retract_atom, and claim_direction."
    }))
}

pub async fn handle_confirm_agent(
    state: &AppState,
    params: Option<Value>,
) -> Result<Value> {
    let params = params.ok_or_else(|| MoteError::Validation("Missing params".to_string()))?;
    
    let agent_id: String = serde_json::from_value(params["agent_id"].clone())
        .map_err(|_| MoteError::Validation("agent_id field required".to_string()))?;
    
    let signature: String = serde_json::from_value(params["signature"].clone())
        .map_err(|_| MoteError::Validation("signature field required".to_string()))?;

    let confirmation = crate::domain::agent::AgentConfirmation { agent_id, signature };
    
    crate::db::queries::confirm_agent(&state.pool, confirmation).await?;
    
    Ok(json!({ "status": "confirmed" }))
}

pub async fn handle_search_atoms(
    state: &AppState,
    params: Option<Value>,
) -> Result<Value> {
    authenticate_and_rate_limit(state, &params).await?;
    let params = params.unwrap_or(json!({}));

    let domain_filter: Option<String> = serde_json::from_value(params["domain"].clone()).ok();
    let type_filter: Option<String> = serde_json::from_value(params["type"].clone()).ok();
    let lifecycle_filter: Option<String> = serde_json::from_value(params["lifecycle"].clone()).ok();
    let text_search: Option<String> = serde_json::from_value(params["query"].clone()).ok();
    let project_id_filter: Option<String> = serde_json::from_value(params["project_id"].clone()).ok();
    let limit: i64 = serde_json::from_value(params["limit"].clone()).unwrap_or(50);
    let offset: i64 = serde_json::from_value(params["offset"].clone()).unwrap_or(0);

    let atoms = crate::db::queries::search_atoms(
        &state.pool,
        domain_filter.as_deref(),
        type_filter.as_deref(),
        lifecycle_filter.as_deref(),
        text_search.as_deref(),
        project_id_filter.as_deref(),
        limit,
        offset,
    ).await?;

    Ok(json!({ "atoms": atoms }))
}

pub async fn handle_query_cluster(
    state: &AppState,
    params: Option<Value>,
) -> Result<Value> {
    authenticate_and_rate_limit(state, &params).await?;
    let params = params.ok_or_else(|| MoteError::Validation("Missing params".to_string()))?;

    // Extract and validate vector
    let vector_value = params.get("vector")
        .and_then(|v| v.as_array())
        .ok_or_else(|| MoteError::Validation("vector field required (array of numbers)".to_string()))?;

    let vector: Vec<f32> = vector_value.iter()
        .map(|v| v.as_f64()
            .ok_or_else(|| MoteError::Validation("vector values must be numbers".to_string()))
            .map(|f| f as f32))
        .collect::<Result<_>>()?;

    if vector.is_empty() {
        return Err(MoteError::Validation("vector must not be empty".to_string()));
    }

    let radius: f64 = serde_json::from_value(params["radius"].clone())
        .map_err(|_| MoteError::Validation("radius field required (number)".to_string()))?;

    let limit: i64 = serde_json::from_value(params["limit"].clone()).unwrap_or(20);
    
    // Enhanced parameters
    let max_hops: u32 = serde_json::from_value(params["max_hops"].clone()).unwrap_or(1);
    let edge_types: Option<Vec<String>> = serde_json::from_value(params["edge_types"].clone()).ok();
    let include_graph_traversal: bool = serde_json::from_value(params["include_graph_traversal"].clone()).unwrap_or(false);
    let cache_key: Option<String> = serde_json::from_value(params["cache_key"].clone()).ok();

    // Check cache first if provided
    if let Some(ref key) = cache_key {
        if let Some(cached_result) = state.graph_cache.read().await.get_cluster_result(key) {
            return Ok(cached_result);
        }
    }

    let results = crate::db::queries::query_cluster_atoms(&state.pool, vector, radius, limit).await?;

    let atoms_json: Vec<Value> = results.iter().map(|r| json!({
        "atom_id":    r.atom.atom_id,
        "atom_type":  r.atom.atom_type.to_string(),
        "domain":     r.atom.domain,
        "statement":  r.atom.statement,
        "conditions": r.atom.conditions,
        "metrics":    r.atom.metrics,
        "distance":   r.distance,
        "pheromone": {
            "attraction":   r.atom.ph_attraction,
            "repulsion":    r.atom.ph_repulsion,
            "novelty":      r.atom.ph_novelty,
            "disagreement": r.atom.ph_disagreement,
        },
        "lifecycle": r.atom.lifecycle.to_string(),
    })).collect();

    // Add graph traversal information if requested
    let graph_traversal = if include_graph_traversal && !results.is_empty() {
        let atom_ids: Vec<String> = results.iter().map(|r| r.atom.atom_id.clone()).collect();
        let traversal_info = crate::db::queries::get_graph_traversal_info(
            &state.pool, &atom_ids, max_hops, edge_types.as_deref()
        ).await.unwrap_or_default();
        
        Some(json!({
            "hops_explored": traversal_info.hops_explored,
            "connected_atoms": traversal_info.connected_atoms,
            "edge_types_found": traversal_info.edge_types_found,
            "traversal_paths": traversal_info.paths
        }))
    } else {
        None
    };

    let pheromone_landscape = if results.is_empty() {
        json!({"attraction": 0.0, "repulsion": 0.0, "novelty": 1.0, "disagreement": 0.0})
    } else {
        let n = results.len() as f64;
        json!({
            "attraction":   results.iter().map(|r| r.atom.ph_attraction).sum::<f64>() / n,
            "repulsion":    results.iter().map(|r| r.atom.ph_repulsion).sum::<f64>() / n,
            "novelty":      results.iter().map(|r| r.atom.ph_novelty).sum::<f64>() / n,
            "disagreement": results.iter().map(|r| r.atom.ph_disagreement).sum::<f64>() / n,
        })
    };

    let mut response = json!({
        "atoms":              atoms_json,
        "pheromone_landscape": pheromone_landscape,
        "total":              results.len(),
        "query_params": {
            "radius": radius,
            "limit": limit,
            "max_hops": max_hops,
            "edge_types": edge_types,
        }
    });

    // Add graph traversal to response if available
    if let Some(traversal) = graph_traversal {
        response["graph_traversal"] = traversal;
    }

    // Cache the result if cache key provided
    if let Some(ref key) = cache_key {
        state.graph_cache.write().await.set_cluster_result(key.clone(), response.clone());
    }

    Ok(response)
}

pub async fn handle_claim_direction(
    state: &AppState,
    params: Option<Value>,
) -> Result<Value> {
    let agent_id = authenticate_and_rate_limit(state, &params).await?;
    let params = params.ok_or_else(|| MoteError::Validation("Missing params".to_string()))?;

    let hypothesis: String = serde_json::from_value(params["hypothesis"].clone())
        .map_err(|_| MoteError::Validation("hypothesis field required".to_string()))?;
    let domain: String = serde_json::from_value(params["domain"].clone())
        .map_err(|_| MoteError::Validation("domain field required".to_string()))?;
    let conditions = if params["conditions"].is_null() {
        json!({})
    } else {
        params["conditions"].clone()
    };
    let parent_ids: Vec<String> = serde_json::from_value(params["parent_ids"].clone())
        .unwrap_or_default();
    let provenance = if parent_ids.is_empty() {
        json!({})
    } else {
        json!({ "parent_ids": parent_ids })
    };

    // Expire stale claims before any read/write
    crate::db::queries::expire_stale_claims(&state.pool).await?;

    // Publish a provisional hypothesis atom for the claimed direction
    let atom_input = crate::domain::atom::AtomInput {
        atom_type: crate::domain::atom::AtomType::Hypothesis,
        domain: domain.clone(),
        project_id: None,
        statement: hypothesis.clone(),
        conditions: conditions.clone(),
        metrics: None,
        provenance,
        signature: vec![],
        artifact_tree_hash: None,
        artifact_inline: None,
    };
    let atom_id = crate::db::queries::publish_atom(&state.pool, &agent_id, atom_input).await?;

    // Update graph cache
    {
        let mut cache = state.graph_cache.write().await;
        cache.add_node(atom_id.clone());
    }

    // Register the claim
    let claim_id = uuid::Uuid::new_v4().to_string();
    let ttl_hours = state.config.workers.claim_ttl_hours as i64;
    let expires_at = chrono::Utc::now() + chrono::Duration::hours(ttl_hours);
    
    // Check for potential claim conflicts before creating
    let potential_conflicts = crate::db::queries::find_potential_claim_conflicts(
        &state.pool, &domain, &hypothesis, &conditions
    ).await.unwrap_or_default();
    
    crate::db::queries::create_claim(&state.pool, &claim_id, &atom_id, &agent_id, expires_at).await?;

    // Gather neighbourhood atoms (same domain, up to 10)
    let neighbourhood = crate::db::queries::get_neighbourhood_atoms(&state.pool, &domain, 10).await?;

    // Gather active claims in this domain
    let active_claims = crate::db::queries::get_active_claims_in_domain(&state.pool, &domain).await?;
    
    // Calculate claim density for this domain
    let claim_density = crate::db::queries::calculate_claim_density(&state.pool, &domain).await.unwrap_or(0.0);

    let neighbourhood_json: Vec<Value> = neighbourhood.iter().map(|a| json!({
        "atom_id":    a.atom_id,
        "atom_type":  a.atom_type.to_string(),
        "domain":     a.domain,
        "statement":  a.statement,
        "conditions": a.conditions,
        "pheromone": {
            "attraction":   a.ph_attraction,
            "repulsion":    a.ph_repulsion,
            "novelty":      a.ph_novelty,
            "disagreement": a.ph_disagreement,
        }
    })).collect();

    let active_claims_json: Vec<Value> = active_claims.iter().map(|c| json!({
        "claim_id":   c.claim_id,
        "atom_id":    c.atom_id,
        "agent_id":   c.agent_id,
        "hypothesis": c.hypothesis,
        "conditions": c.conditions,
        "expires_at": c.expires_at.to_rfc3339(),
    })).collect();

    // Aggregate pheromone landscape for the domain (enhanced with claim density)
    let pheromone_landscape = if neighbourhood.is_empty() {
        json!({
            "attraction": 0.0, 
            "repulsion": 0.0, 
            "novelty": 1.0, 
            "disagreement": 0.0,
            "claim_density": claim_density
        })
    } else {
        let n = neighbourhood.len() as f64;
        json!({
            "attraction":   neighbourhood.iter().map(|a| a.ph_attraction).sum::<f64>() / n,
            "repulsion":    neighbourhood.iter().map(|a| a.ph_repulsion).sum::<f64>() / n,
            "novelty":      neighbourhood.iter().map(|a| a.ph_novelty).sum::<f64>() / n,
            "disagreement": neighbourhood.iter().map(|a| a.ph_disagreement).sum::<f64>() / n,
            "claim_density": claim_density
        })
    };
    
    // Format potential conflicts for response
    let conflicts_json: Vec<Value> = potential_conflicts.iter().map(|c| json!({
        "claim_id": c.claim_id,
        "agent_id": c.agent_id,
        "hypothesis": c.hypothesis,
        "conditions": c.conditions,
        "similarity_score": c.similarity_score,
        "conflict_type": c.conflict_type
    })).collect();

    Ok(json!({
        "atom_id":             atom_id,
        "claim_id":            claim_id,
        "expires_at":          expires_at.to_rfc3339(),
        "neighbourhood":       neighbourhood_json,
        "active_claims":       active_claims_json,
        "pheromone_landscape": pheromone_landscape,
        "potential_conflicts": conflicts_json,
        "claim_density":       claim_density,
        "warnings":           if !conflicts_json.is_empty() { 
                                vec![format!("{} potential claim conflicts detected", conflicts_json.len())] 
                              } else { 
                                vec![] 
                              }
    }))
}

pub async fn handle_publish_atoms(
    state: &AppState,
    params: Option<Value>,
) -> Result<Value> {
    // Auth and rate limiting for mutating methods
    let agent_id = authenticate_and_rate_limit(state, &params).await?;

    let params = params.ok_or_else(|| MoteError::Validation("Missing params".to_string()))?;

    let atoms_value: Value = serde_json::from_value(params["atoms"].clone())
        .map_err(|_| MoteError::Validation("atoms field required".to_string()))?;

    let atoms_array = atoms_value.as_array()
        .ok_or_else(|| MoteError::Validation("atoms must be an array".to_string()))?;

    let mut published_atoms: Vec<String> = Vec::new();
    let mut pheromone_deltas: Vec<Value> = Vec::new();
    let mut auto_contradictions: Vec<Value> = Vec::new();

    for atom_value in atoms_array {
        let atom_type_str: String = serde_json::from_value(atom_value["atom_type"].clone())
            .map_err(|_| MoteError::Validation("atom_type field required".to_string()))?;

        let atom_type = match atom_type_str.as_str() {
            "hypothesis" => crate::domain::atom::AtomType::Hypothesis,
            "finding" => crate::domain::atom::AtomType::Finding,
            "negative_result" => crate::domain::atom::AtomType::NegativeResult,
            "delta" => crate::domain::atom::AtomType::Delta,
            "experiment_log" => crate::domain::atom::AtomType::ExperimentLog,
            "synthesis" => crate::domain::atom::AtomType::Synthesis,
            "bounty" => crate::domain::atom::AtomType::Bounty,
            _ => return Err(MoteError::Validation(format!("Unknown atom type: {}", atom_type_str))),
        };

        let domain: String = serde_json::from_value(atom_value["domain"].clone())
            .map_err(|_| MoteError::Validation("domain field required".to_string()))?;

        let conditions = if atom_value["conditions"].is_null() {
            json!({})
        } else {
            atom_value["conditions"].clone()
        };

        let provenance = if atom_value["provenance"].is_null() {
            json!({})
        } else {
            atom_value["provenance"].clone()
        };

        let metrics = if atom_value["metrics"].is_null() {
            None
        } else {
            Some(atom_value["metrics"].clone())
        };

        let artifact_tree_hash: Option<String> = serde_json::from_value(atom_value["artifact_tree_hash"].clone())
            .unwrap_or(None);

        let artifact_inline: Option<crate::api::artifact_processor::InlineArtifact> = 
            serde_json::from_value(atom_value["artifact_inline"].clone())
            .unwrap_or(None);

        // Process inline artifact if present
        let final_artifact_hash = if let Some(artifact) = artifact_inline {
            let artifact_hash = crate::api::artifact_processor::process_inline_artifact(
                &state.pool, &*state.storage, &agent_id, artifact
            ).await?;
            Some(artifact_hash)
        } else {
            artifact_tree_hash
        };

        // TODO: SECURITY — verify atom signature against the author's Ed25519 public key.
        // Implementation path:
        //   1. SELECT public_key FROM agents WHERE agent_id = $agent_id
        //   2. Define a canonical signed message (e.g. SHA-256 of JSON-encoded immutable fields)
        //   3. Call crate::crypto::signing::verify_signature(&public_key, &message, &signature)
        //   4. Return 403 if verification fails
        // Currently skipped because:
        //   (a) Agents may omit the signature field (signature=[]) — enforcing would be breaking.
        //   (b) The canonical signed-message format is not yet specified in the protocol.
        // Before public/untrusted deployment: define the message format and require non-empty sigs.
        let atom_signature: Vec<u8> = match atom_value.get("signature") {
            None | Some(Value::Null) => vec![],
            Some(Value::String(s)) => {
                crate::crypto::signing::hex_to_bytes(s).unwrap_or_default()
            }
            Some(v) => serde_json::from_value::<Vec<u8>>(v.clone()).unwrap_or_default(),
        };

        let statement: String = serde_json::from_value(atom_value["statement"].clone())
            .map_err(|_| MoteError::Validation("statement field required".to_string()))?;

        let project_id: Option<String> = serde_json::from_value(atom_value["project_id"].clone())
            .unwrap_or(None);

        let atom_input = crate::domain::atom::AtomInput {
            atom_type: atom_type.clone(),
            domain: domain.clone(),
            project_id,
            statement,
            conditions: conditions.clone(),
            metrics: metrics.clone(),
            provenance,
            signature: atom_signature,
            artifact_tree_hash: final_artifact_hash,
            artifact_inline: None, // Processed inline artifacts are stored as tree_hash
        };

        let atom_id = crate::db::queries::publish_atom(&state.pool, &agent_id, atom_input).await?;

        // Auto-register any new condition keys for this domain (no-op if already present)
        auto_register_conditions(&state.pool, &domain, &conditions).await;

        // Update graph cache incrementally
        {
            let mut cache = state.graph_cache.write().await;
            cache.add_node(atom_id.clone());
        }

        // Emit atom_published SSE event (fire-and-forget; ignore if no subscribers)
        let _ = state.sse_broadcast_tx.send(crate::state::SseEvent {
            event_type: "atom_published".to_string(),
            data: json!({
                "type":      "atom_published",
                "atom_id":   atom_id,
                "domain":    domain,
                "atom_type": atom_type.to_string(),
            }),
            timestamp: chrono::Utc::now(),
        });

        // ── Pheromone + contradiction updates ────────────────────────────────
        let is_evidence = matches!(atom_type,
            crate::domain::atom::AtomType::Finding |
            crate::domain::atom::AtomType::NegativeResult);

        let mut attraction_delta = 0.0_f64;
        let mut disagreement_delta = 0.0_f64;

        if is_evidence {
            // Detect contradictions before bumping pheromone
            let contradictions = crate::db::queries::find_contradicting_atoms(
                &state.pool, &domain, &atom_id, &conditions, &metrics,
            ).await?;

            for c in &contradictions {
                // Bump disagreement on both atoms
                crate::db::queries::update_pheromone_disagreement(
                    &state.pool, &atom_id, &c.existing_atom_id,
                ).await?;
                disagreement_delta += 0.1;

                auto_contradictions.push(json!({
                    "new_atom_id":        atom_id,
                    "existing_atom_id":   c.existing_atom_id,
                    "existing_statement": c.existing_statement,
                    "conflicting_metrics": c.conflicting_metrics,
                }));

                let _ = state.sse_broadcast_tx.send(crate::state::SseEvent {
                    event_type: "contradiction_detected".to_string(),
                    data: json!({
                        "type":                 "contradiction_detected",
                        "atom_id":              atom_id,
                        "contradicting_atom_id": c.existing_atom_id,
                        "domain":               domain,
                    }),
                    timestamp: chrono::Utc::now(),
                });
            }

            // For findings with positive metrics, bump attraction in the neighbourhood.
            // Use 0.1 as a fixed delta (proper metric-improvement scoring is a future enhancement).
            if matches!(atom_type, crate::domain::atom::AtomType::Finding) {
                crate::db::queries::update_pheromone_attraction(
                    &state.pool, &domain, &atom_id, 0.1,
                ).await?;
                attraction_delta = 0.1;
            }
        }

        pheromone_deltas.push(json!({
            "atom_id":           atom_id.clone(),
            "attraction_delta":  attraction_delta,
            "disagreement_delta": disagreement_delta,
        }));

        published_atoms.push(atom_id);
    }

    // Insert explicit edges passed with this publish request
    if let Some(edges_array) = params["edges"].as_array() {
        for edge in edges_array {
            let src  = edge["source_atom_id"].as_str().unwrap_or("");
            let tgt  = edge["target_atom_id"].as_str().unwrap_or("");
            let etype = edge["edge_type"].as_str().unwrap_or("");
            if src.is_empty() || tgt.is_empty() || etype.is_empty() { continue; }
            if src == tgt { continue; } // reject self-referential edges
            let valid = matches!(etype, "derived_from"|"inspired_by"|"contradicts"|"replicates"|"summarizes"|"supersedes"|"retracts");
            if !valid { continue; }
            if sqlx::query(
                "INSERT INTO edges (source_id, target_id, type) VALUES ($1,$2,$3)
                 ON CONFLICT (source_id, target_id, type) DO NOTHING"
            )
            .bind(src).bind(tgt).bind(etype)
            .execute(&state.pool)
            .await
            .map(|r| r.rows_affected() > 0)
            .unwrap_or(false)
            {
                // Reset decay clock on both atoms — a new edge is a sign of activity.
                sqlx::query(
                    "UPDATE atoms SET last_activity_at = NOW() WHERE atom_id IN ($1, $2)"
                )
                .bind(src).bind(tgt)
                .execute(&state.pool)
                .await
                .ok();
            }
        }
    }

    // Also derive edges from parent_ids in each atom's provenance
    for atom_id in &published_atoms {
        // Re-fetch the provenance we stored so we can extract parent_ids
        if let Ok(row) = sqlx::query("SELECT provenance FROM atoms WHERE atom_id = $1")
            .bind(atom_id)
            .fetch_one(&state.pool)
            .await
        {
            let prov: serde_json::Value = row.get("provenance");
            if let Some(parent_ids) = prov["parent_ids"].as_array() {
                for pid in parent_ids {
                    if let Some(pid_str) = pid.as_str() {
                        if sqlx::query(
                            "INSERT INTO edges (source_id, target_id, type) VALUES ($1,$2,'derived_from')
                             ON CONFLICT (source_id, target_id, type) DO NOTHING"
                        )
                        .bind(atom_id).bind(pid_str)
                        .execute(&state.pool)
                        .await
                        .map(|r| r.rows_affected() > 0)
                        .unwrap_or(false)
                        {
                            sqlx::query(
                                "UPDATE atoms SET last_activity_at = NOW() WHERE atom_id IN ($1, $2)"
                            )
                            .bind(atom_id).bind(pid_str)
                            .execute(&state.pool)
                            .await
                            .ok();
                        }
                    }
                }
            }
        }
    }

    // Increment publish requests accepted counter
    state.metrics.publish_requests_accepted.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    state.metrics.publish_requests_queued.fetch_add(published_atoms.len() as u64, std::sync::atomic::Ordering::Relaxed);

    Ok(json!({
        "published_atoms":    published_atoms,
        "pheromone_deltas":   pheromone_deltas,
        "auto_contradictions": auto_contradictions,
    }))
}

pub async fn handle_retract_atom(
    state: &AppState,
    params: Option<Value>,
) -> Result<Value> {
    // Auth and rate limiting for mutating methods
    let agent_id = authenticate_and_rate_limit(state, &params).await?;
    
    let params = params.ok_or_else(|| MoteError::Validation("Missing params".to_string()))?;
    
    let atom_id: String = serde_json::from_value(params["atom_id"].clone())
        .map_err(|_| MoteError::Validation("atom_id field required".to_string()))?;
    
    let reason: Option<String> = serde_json::from_value(params["reason"].clone()).ok();

    crate::db::queries::retract_atom(&state.pool, &atom_id, &agent_id, reason.as_deref()).await?;

    Ok(json!({ "status": "retracted" }))
}

pub async fn handle_ban_atom(
    state: &AppState,
    params: Option<Value>,
) -> Result<Value> {
    authenticate_and_rate_limit(state, &params).await?;
    let params = params.ok_or_else(|| MoteError::Validation("Missing params".to_string()))?;
    let atom_id: String = serde_json::from_value(params["atom_id"].clone())
        .map_err(|_| MoteError::Validation("atom_id field required".to_string()))?;
    crate::db::queries::ban_atom(&state.pool, &atom_id).await?;
    Ok(json!({ "status": "banned" }))
}

pub async fn handle_unban_atom(
    state: &AppState,
    params: Option<Value>,
) -> Result<Value> {
    authenticate_and_rate_limit(state, &params).await?;
    let params = params.ok_or_else(|| MoteError::Validation("Missing params".to_string()))?;
    let atom_id: String = serde_json::from_value(params["atom_id"].clone())
        .map_err(|_| MoteError::Validation("atom_id field required".to_string()))?;
    crate::db::queries::unban_atom(&state.pool, &atom_id).await?;
    Ok(json!({ "status": "unbanned" }))
}

pub async fn handle_get_suggestions(
    state: &AppState,
    params: Option<Value>,
) -> Result<Value> {
    authenticate_and_rate_limit(state, &params).await?;
    let params = params.ok_or_else(|| MoteError::Validation("Missing params".to_string()))?;
    
    // Extract optional filters
    let domain_filter: Option<String> = serde_json::from_value(params["domain"].clone()).ok();
    let limit: i64 = serde_json::from_value(params["limit"].clone()).unwrap_or(10);
    let include_exploration: bool = serde_json::from_value(params["include_exploration"].clone()).unwrap_or(false);
    
    // Debug logging
    tracing::info!("get_suggestions called with domain_filter={:?}, limit={}, include_exploration={}", 
                   domain_filter, limit, include_exploration);
    tracing::info!("Raw params: {}", serde_json::to_string(&params).unwrap_or_default());
    
    let mut all_suggestions = Vec::new();
    
    // Always get pheromone-based suggestions
    let pheromone_suggestions = get_pheromone_suggestions(&state.pool, &domain_filter, limit).await?;
    all_suggestions.extend(pheromone_suggestions);
    
    // Add exploration suggestions if requested
    if include_exploration {
        tracing::info!("Adding exploration suggestions...");
        let exploration_suggestions = get_exploration_suggestions(
            state, 
            &domain_filter, 
            limit
        ).await?;
        let exploration_count = exploration_suggestions.len();
        all_suggestions.extend(exploration_suggestions);
        tracing::info!("Added {} exploration suggestions", exploration_count);
    } else {
        tracing::info!("Exploration mode not requested");
    }
    
    // Determine strategy
    let strategy = if include_exploration {
        "pheromone_attraction_plus_exploration"
    } else {
        "pheromone_attraction"
    };
    
    tracing::info!("Final strategy: {}", strategy);
    
    let description = if include_exploration {
        "Atoms ranked by pheromone attraction plus exploration sampling (high novelty/disagreement potential)"
    } else {
        "Atoms ranked by pheromone attraction (high novelty/disagreement potential)"
    };
    
    // Add debug info to response
    let debug_info = json!({
        "debug_include_exploration": include_exploration,
        "debug_domain_filter": domain_filter,
        "debug_limit": limit,
        "debug_all_suggestions_count": all_suggestions.len()
    });
    
    Ok(json!({ 
        "suggestions": all_suggestions,
        "strategy": strategy,
        "description": description,
        "debug": debug_info
    }))
}

pub async fn get_pheromone_suggestions(
    pool: &sqlx::PgPool,
    domain_filter: &Option<String>,
    limit: i64,
) -> Result<Vec<Value>> {
    // Score formula: novelty × (1+disagreement) × attraction / (1+repulsion) / (1+active_claims)
    // Active-claim dampening: each claim on the atom reduces effective attraction, preventing
    // stampedes where many agents converge on the same direction simultaneously.
    let rows = if let Some(domain) = domain_filter {
        sqlx::query(
            "SELECT a.atom_id, a.type, a.domain, a.statement, a.conditions, a.metrics,
             a.ph_attraction, a.ph_repulsion, a.ph_novelty, a.ph_disagreement,
             COALESCE(c.claim_count, 0)::bigint AS claim_count,
             (a.ph_novelty * (1.0 + a.ph_disagreement) * a.ph_attraction
              / (1.0 + a.ph_repulsion)
              / (1.0 + COALESCE(c.claim_count, 0))) AS score
             FROM atoms a
             LEFT JOIN (
               SELECT atom_id, COUNT(*) AS claim_count
               FROM claims WHERE active = true
               GROUP BY atom_id
             ) c ON a.atom_id = c.atom_id
             WHERE NOT a.archived
             AND a.ph_attraction >= 0
             AND a.domain = $1
             ORDER BY score DESC
             LIMIT $2"
        )
        .bind(domain)
        .bind(limit)
        .fetch_all(pool)
        .await
        .map_err(MoteError::Database)?
    } else {
        sqlx::query(
            "SELECT a.atom_id, a.type, a.domain, a.statement, a.conditions, a.metrics,
             a.ph_attraction, a.ph_repulsion, a.ph_novelty, a.ph_disagreement,
             COALESCE(c.claim_count, 0)::bigint AS claim_count,
             (a.ph_novelty * (1.0 + a.ph_disagreement) * a.ph_attraction
              / (1.0 + a.ph_repulsion)
              / (1.0 + COALESCE(c.claim_count, 0))) AS score
             FROM atoms a
             LEFT JOIN (
               SELECT atom_id, COUNT(*) AS claim_count
               FROM claims WHERE active = true
               GROUP BY atom_id
             ) c ON a.atom_id = c.atom_id
             WHERE NOT a.archived
             AND a.ph_attraction >= 0
             ORDER BY score DESC
             LIMIT $1"
        )
        .bind(limit)
        .fetch_all(pool)
        .await
        .map_err(MoteError::Database)?
    };

    let mut suggestions = Vec::new();

    for row in rows {
        let claim_count = row.get::<i64, _>("claim_count");
        let score = row.get::<f64, _>("score");
        let suggestion = json!({
            "atom_id": row.get::<String, _>("atom_id"),
            "atom_type": row.get::<String, _>("type"),
            "domain": row.get::<String, _>("domain"),
            "statement": row.get::<String, _>("statement"),
            "conditions": row.get::<serde_json::Value, _>("conditions"),
            "metrics": row.get::<Option<serde_json::Value>, _>("metrics"),
            "source": "pheromone",
            "score": score,
            "active_claims": claim_count,
            "pheromone": {
                "attraction": row.get::<f32, _>("ph_attraction"),
                "repulsion": row.get::<f32, _>("ph_repulsion"),
                "novelty": row.get::<f32, _>("ph_novelty"),
                "disagreement": row.get::<f32, _>("ph_disagreement")
            }
        });

        suggestions.push(suggestion);
    }

    Ok(suggestions)
}

pub async fn get_exploration_suggestions(
    state: &crate::state::AppState,
    _domain_filter: &Option<String>,
    limit: i64,
) -> Result<Vec<Value>> {
    use rand::Rng;
    
    let exploration_samples = state.config.pheromone.exploration_samples;
    let exploration_radius = state.config.pheromone.exploration_density_radius;
    let embedding_dimension = state.config.hub.embedding_dimension;
    
    let mut exploration_suggestions = Vec::new();
    
    for _ in 0..exploration_samples {
        // Generate random unit vector — rng scoped to block so it drops before .await below
        let random_vector: Vec<f32> = {
            use rand::SeedableRng;
            let mut rng = rand::rngs::StdRng::from_os_rng();
            (0..embedding_dimension)
                .map(|_| rng.random_range(-1.0f32..1.0f32))
                .collect()
        };
        
        // Normalize to unit vector
        let norm: f32 = random_vector.iter().map(|x| x * x).sum::<f32>().sqrt();
        if norm > 0.0 {
            let unit_vector: Vec<f32> = random_vector.iter().map(|x| x / norm).collect();
            
            // Query nearest atom with density
            if let Ok((Some(nearest_atom), atom_count)) = crate::db::queries::query_nearest_atom_with_density(
                &state.pool,
                unit_vector,
                exploration_radius
            ).await {
                // Compute novelty score
                let novelty = 1.0 / (1.0 + atom_count as f64);
                
                // Only include if novelty is meaningful
                if novelty > 0.5 {
                    let suggestion = json!({
                        "atom_id": nearest_atom.atom_id,
                        "atom_type": nearest_atom.atom_type.to_string(),
                        "domain": nearest_atom.domain,
                        "statement": nearest_atom.statement,
                        "conditions": nearest_atom.conditions,
                        "metrics": nearest_atom.metrics,
                        "source": "exploration",
                        "novelty": novelty,
                        "atom_count": atom_count,
                        "pheromone": {
                            "attraction": nearest_atom.ph_attraction,
                            "repulsion": nearest_atom.ph_repulsion,
                            "novelty": nearest_atom.ph_novelty,
                            "disagreement": nearest_atom.ph_disagreement
                        }
                    });
                    
                    exploration_suggestions.push(suggestion);
                }
            }
        }
    }
    
    // Sort by novelty descending and limit
    exploration_suggestions.sort_by(|a, b| {
        let novelty_a = a["novelty"].as_f64().unwrap_or(0.0);
        let novelty_b = b["novelty"].as_f64().unwrap_or(0.0);
        novelty_b.partial_cmp(&novelty_a).unwrap_or(std::cmp::Ordering::Equal)
    });
    
    exploration_suggestions.truncate(limit as usize);
    Ok(exploration_suggestions)
}

pub async fn handle_get_graph_edges(state: &AppState, params: Option<Value>) -> Result<Value> {
    // Only authenticate when called from the public RPC endpoint (params present).
    // Internal callers (rspc_router) pass None to bypass.
    if params.is_some() {
        authenticate_and_rate_limit(state, &params).await?;
    }
    let rows = sqlx::query(
        "SELECT e.source_id, e.target_id, e.type, e.repl_type, e.created_at
         FROM edges e
         JOIN atoms a1 ON e.source_id = a1.atom_id AND NOT a1.retracted AND NOT a1.archived
         JOIN atoms a2 ON e.target_id = a2.atom_id AND NOT a2.retracted AND NOT a2.archived
         ORDER BY e.created_at ASC"
    )
    .fetch_all(&state.pool)
    .await
    .map_err(MoteError::Database)?;

    let edges: Vec<Value> = rows.iter().map(|r| {
        let repl_type: Option<String> = r.get("repl_type");
        json!({
            "source_id":  r.get::<String, _>("source_id"),
            "target_id":  r.get::<String, _>("target_id"),
            "edge_type":  r.get::<String, _>("type"),
            "repl_type":  repl_type,
            "created_at": r.get::<chrono::DateTime<chrono::Utc>, _>("created_at"),
        })
    }).collect();

    Ok(json!({ "edges": edges, "count": edges.len() }))
}

pub async fn handle_get_field_map(
    state: &AppState,
    params: Option<Value>,
) -> Result<Value> {
    authenticate_and_rate_limit(state, &params).await?;
    let domain_filter = params
        .as_ref()
        .and_then(|p| p.get("domain"))
        .and_then(|v| v.as_str())
        .map(|s| s.to_string());
    
    let atoms = crate::db::queries::get_synthesis_atoms(
        &state.pool,
        domain_filter.as_deref(),
    ).await?;
    
    let result: Vec<Value> = atoms.into_iter().map(|atom| {
        json!({
            "atom_id": atom.atom_id,
            "type": atom.atom_type.to_string(),
            "domain": atom.domain,
            "statement": atom.statement,
            "conditions": atom.conditions,
            "metrics": atom.metrics,
            "provenance": atom.provenance,
            "author_agent_id": atom.author_agent_id,
            "created_at": atom.created_at.to_rfc3339(),
            "confidence": atom.confidence,
            "ph_attraction": atom.ph_attraction,
            "ph_repulsion": atom.ph_repulsion,
            "ph_novelty": atom.ph_novelty,
            "ph_disagreement": atom.ph_disagreement,
            "embedding": atom.embedding,
            "embedding_status": atom.embedding_status.to_string(),
            "repl_exact": atom.repl_exact,
            "repl_conceptual": atom.repl_conceptual,
            "repl_extension": atom.repl_extension,
            "traffic": atom.traffic,
            "lifecycle": atom.lifecycle.to_string(),
            "retracted": atom.retracted,
            "retraction_reason": atom.retraction_reason,
            "ban_flag": atom.ban_flag,
            "archived": atom.archived,
            "probationary": atom.probationary,
            "summary": atom.summary,
        })
    }).collect();
    
    Ok(json!({
        "atoms": result,
        "count": result.len()
    }))
}

/// Infer a condition_registry value_type from a JSON value.
fn infer_value_type(v: &serde_json::Value) -> &'static str {
    match v {
        serde_json::Value::Number(n) => {
            if n.is_f64() && n.as_f64().map(|f| f.fract() != 0.0).unwrap_or(false) {
                "float"
            } else {
                "int"
            }
        }
        serde_json::Value::Bool(_) => "string",
        _ => "string",
    }
}

/// Upsert condition keys for a domain into condition_registry on first observation.
/// All auto-discovered keys start as required=false; humans can promote them later.
/// Errors are silently ignored — this is best-effort bookkeeping.
async fn auto_register_conditions(pool: &sqlx::PgPool, domain: &str, conditions: &serde_json::Value) {
    let Some(obj) = conditions.as_object() else { return };
    if obj.is_empty() { return }

    for (key, val) in obj {
        let vtype = infer_value_type(val);
        let _ = sqlx::query(
            "INSERT INTO condition_registry (domain, key_name, value_type, unit, required)
             VALUES ($1, $2, $3, NULL, false)
             ON CONFLICT (domain, key_name) DO NOTHING"
        )
        .bind(domain)
        .bind(key)
        .bind(vtype)
        .execute(pool)
        .await;
    }
}
