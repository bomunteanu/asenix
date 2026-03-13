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
) -> std::result::Result<Json<JsonRpcResponse>, (axum::http::StatusCode, String)> {
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
        return Ok(Json(JsonRpcResponse {
            jsonrpc: "2.0".to_string(),
            result: None,
            error: Some(JsonRpcError {
                code: -32700,
                message: "Batch requests not supported".to_string(),
                data: Some(json!({
                    "request_id": request_id,
                    "timestamp": chrono::Utc::now().to_rfc3339()
                })),
            }),
            id: None,
        }));
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
        return Ok(Json(JsonRpcResponse {
            jsonrpc: "2.0".to_string(),
            result: None,
            error: Some(JsonRpcError {
                code: -32600,
                message: "Invalid JSON-RPC version".to_string(),
                data: Some(json!({
                    "request_id": request_id,
                    "timestamp": chrono::Utc::now().to_rfc3339()
                })),
            }),
            id: request.id,
        }));
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
        "get_suggestions" => handle_get_suggestions(&state, request.params).await,
        "get_field_map" => handle_get_field_map(&state, request.params).await,
        _ => Err(MoteError::Validation(format!("Method not found: {}", request.method))),
    };

    // Format response
    match result {
        Ok(result_value) => Ok(Json(JsonRpcResponse {
            jsonrpc: "2.0".to_string(),
            result: Some(result_value),
            error: None,
            id: request.id,
        })),
        Err(error) => Ok(Json(JsonRpcResponse {
            jsonrpc: "2.0".to_string(),
            result: None,
            error: Some(JsonRpcError {
                code: error.json_rpc_code(),
                message: error.to_string(),
                data: Some(json!({
                    "request_id": request_id,
                    "timestamp": chrono::Utc::now().to_rfc3339(),
                    "error_type": match error {
                        MoteError::RateLimit => "rate_limit",
                        MoteError::Authentication(_) => "authentication",
                        MoteError::Validation(_) => "validation",
                        MoteError::NotFound(_) => "not_found",
                        MoteError::Conflict(_) => "conflict",
                        MoteError::ExternalService(_) => "external_service",
                        MoteError::Internal(_) => "internal",
                        MoteError::Database(_) => "database",
                        MoteError::Serialization(_) => "serialization",
                        MoteError::Configuration(_) => "configuration",
                        MoteError::Cryptography(_) => "cryptography",
                    }
                })),
            }),
            id: request.id,
        })),
    }
}

// Authentication and rate limiting for mutating methods.
// Accepts either token-based auth (api_token field) or Ed25519 signature-based auth
// (signature field). Token-based auth is the recommended path for AI agents.
async fn authenticate_and_rate_limit(
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
    let params = params.unwrap_or(json!({}));

    let domain_filter: Option<String> = serde_json::from_value(params["domain"].clone()).ok();
    let type_filter: Option<String> = serde_json::from_value(params["type"].clone()).ok();
    let lifecycle_filter: Option<String> = serde_json::from_value(params["lifecycle"].clone()).ok();
    let text_search: Option<String> = serde_json::from_value(params["query"].clone()).ok();
    let limit: i64 = serde_json::from_value(params["limit"].clone()).unwrap_or(50);
    let offset: i64 = serde_json::from_value(params["offset"].clone()).unwrap_or(0);

    let atoms = crate::db::queries::search_atoms(
        &state.pool,
        domain_filter.as_deref(),
        type_filter.as_deref(),
        lifecycle_filter.as_deref(),
        text_search.as_deref(),
        limit,
        offset,
    ).await?;

    Ok(json!({ "atoms": atoms }))
}

pub async fn handle_query_cluster(
    state: &AppState,
    params: Option<Value>,
) -> Result<Value> {
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

    Ok(json!({
        "atoms":              atoms_json,
        "pheromone_landscape": pheromone_landscape,
        "total":              results.len(),
    }))
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

    // Expire stale claims before any read/write
    crate::db::queries::expire_stale_claims(&state.pool).await?;

    // Publish a provisional hypothesis atom for the claimed direction
    let atom_input = crate::domain::atom::AtomInput {
        atom_type: crate::domain::atom::AtomType::Hypothesis,
        domain: domain.clone(),
        statement: hypothesis.clone(),
        conditions: conditions.clone(),
        metrics: None,
        provenance: json!({}),
        signature: vec![],
        artifact_tree_hash: None,
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
    crate::db::queries::create_claim(&state.pool, &claim_id, &atom_id, &agent_id, expires_at).await?;

    // Gather neighbourhood atoms (same domain, up to 10)
    let neighbourhood = crate::db::queries::get_neighbourhood_atoms(&state.pool, &domain, 10).await?;

    // Gather active claims in this domain
    let active_claims = crate::db::queries::get_active_claims_in_domain(&state.pool, &domain).await?;

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

    // Aggregate pheromone landscape for the domain
    let pheromone_landscape = if neighbourhood.is_empty() {
        json!({"attraction": 0.0, "repulsion": 0.0, "novelty": 1.0, "disagreement": 0.0})
    } else {
        let n = neighbourhood.len() as f64;
        json!({
            "attraction":   neighbourhood.iter().map(|a| a.ph_attraction).sum::<f64>() / n,
            "repulsion":    neighbourhood.iter().map(|a| a.ph_repulsion).sum::<f64>() / n,
            "novelty":      neighbourhood.iter().map(|a| a.ph_novelty).sum::<f64>() / n,
            "disagreement": neighbourhood.iter().map(|a| a.ph_disagreement).sum::<f64>() / n,
        })
    };

    Ok(json!({
        "atom_id":             atom_id,
        "claim_id":            claim_id,
        "expires_at":          expires_at.to_rfc3339(),
        "neighbourhood":       neighbourhood_json,
        "active_claims":       active_claims_json,
        "pheromone_landscape": pheromone_landscape,
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

        // Per-atom signature is stored but not verified; accept hex string, raw bytes,
        // or absent (token-auth agents don't need to sign individual atoms).
        let atom_signature: Vec<u8> = match atom_value.get("signature") {
            None | Some(Value::Null) => vec![],
            Some(Value::String(s)) => {
                crate::crypto::signing::hex_to_bytes(s).unwrap_or_default()
            }
            Some(v) => serde_json::from_value::<Vec<u8>>(v.clone()).unwrap_or_default(),
        };

        let statement: String = serde_json::from_value(atom_value["statement"].clone())
            .map_err(|_| MoteError::Validation("statement field required".to_string()))?;

        let atom_input = crate::domain::atom::AtomInput {
            atom_type: atom_type.clone(),
            domain: domain.clone(),
            statement,
            conditions: conditions.clone(),
            metrics: metrics.clone(),
            provenance,
            signature: atom_signature,
            artifact_tree_hash,
        };

        let atom_id = crate::db::queries::publish_atom(&state.pool, &agent_id, atom_input).await?;

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

pub async fn handle_get_suggestions(
    state: &AppState,
    params: Option<Value>,
) -> Result<Value> {
    let params = params.ok_or_else(|| MoteError::Validation("Missing params".to_string()))?;
    
    // Extract optional filters
    let domain_filter: Option<String> = serde_json::from_value(params["domain"].clone()).ok();
    let limit: i64 = serde_json::from_value(params["limit"].clone()).unwrap_or(10);
    
    // Query atoms with highest pheromone attraction (suggesting high novelty/disagreement)
    let rows = if let Some(domain) = domain_filter {
        sqlx::query(
            "SELECT atom_id, type, domain, statement, conditions, metrics, 
             ph_attraction, ph_repulsion, ph_novelty, ph_disagreement
             FROM atoms 
             WHERE NOT archived 
             AND ph_attraction >= 0
             AND domain = $1
             ORDER BY ph_attraction DESC, ph_novelty DESC 
             LIMIT $2"
        )
        .bind(&domain)
        .bind(limit)
        .fetch_all(&state.pool)
        .await
        .map_err(MoteError::Database)?
    } else {
        sqlx::query(
            "SELECT atom_id, type, domain, statement, conditions, metrics, 
             ph_attraction, ph_repulsion, ph_novelty, ph_disagreement
             FROM atoms 
             WHERE NOT archived 
             AND ph_attraction >= 0
             ORDER BY ph_attraction DESC, ph_novelty DESC 
             LIMIT $1"
        )
        .bind(limit)
        .fetch_all(&state.pool)
        .await
        .map_err(MoteError::Database)?
    };
    
    let mut suggestions = Vec::new();
    
    for row in rows {
        let suggestion = json!({
            "atom_id": row.get::<String, _>("atom_id"),
            "atom_type": row.get::<String, _>("type"),
            "domain": row.get::<String, _>("domain"),
            "statement": row.get::<String, _>("statement"),
            "conditions": row.get::<serde_json::Value, _>("conditions"),
            "metrics": row.get::<Option<serde_json::Value>, _>("metrics"),
            "pheromone": {
                "attraction": row.get::<f32, _>("ph_attraction"),
                "repulsion": row.get::<f32, _>("ph_repulsion"),
                "novelty": row.get::<f32, _>("ph_novelty"),
                "disagreement": row.get::<f32, _>("ph_disagreement")
            }
        });
        
        suggestions.push(suggestion);
    }
    
    Ok(json!({ 
        "suggestions": suggestions,
        "strategy": "pheromone_attraction",
        "description": "Atoms ranked by pheromone attraction (high novelty/disagreement potential)"
    }))
}

pub async fn handle_get_field_map(
    state: &AppState,
    params: Option<Value>,
) -> Result<Value> {
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
