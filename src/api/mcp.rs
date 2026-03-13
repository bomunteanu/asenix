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

// Authentication and rate limiting for mutating methods
async fn authenticate_and_rate_limit(
    state: &AppState,
    params: &Option<Value>,
) -> Result<String> {
    let params = params.as_ref().ok_or_else(|| MoteError::Validation("Missing params".to_string()))?;

    // Extract agent_id and signature
    let agent_id: String = serde_json::from_value(params["agent_id"].clone())
        .map_err(|_| MoteError::Validation("agent_id field required".to_string()))?;
    
    let signature: String = serde_json::from_value(params["signature"].clone())
        .map_err(|_| MoteError::Validation("signature field required".to_string()))?;

    // Load agent and verify confirmed
    let agent = crate::db::queries::get_agent(&state.pool, &agent_id).await
        .map_err(|_| MoteError::Authentication("Agent not found".to_string()))?
        .ok_or_else(|| MoteError::Authentication("Agent not found".to_string()))?;
    
    if !agent.confirmed {
        return Err(MoteError::Authentication("Agent not confirmed".to_string()));
    }

    // Verify signature
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

    // Rate limiting
    let _request_count = if params["atoms"].is_array() {
        params["atoms"].as_array().unwrap().len()
    } else {
        1
    };
    
    if !state.rate_limiter.check_rate_limit(&agent_id, state.config.trust.max_atoms_per_hour) {
        // Increment rate limit rejection counter
        state.metrics.rate_limit_rejections.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        return Err(MoteError::RateLimit);
    }

    Ok(agent_id)
}

// Handler implementations
async fn handle_register_agent(
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

async fn handle_confirm_agent(
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

async fn handle_search_atoms(
    state: &AppState,
    params: Option<Value>,
) -> Result<Value> {
    let params = params.ok_or_else(|| MoteError::Validation("Missing params".to_string()))?;
    
    let domain_filter: Option<String> = serde_json::from_value(params["domain"].clone()).ok();
    let type_filter: Option<String> = serde_json::from_value(params["type"].clone()).ok();
    let lifecycle_filter: Option<String> = serde_json::from_value(params["lifecycle"].clone()).ok();
    let limit: i64 = serde_json::from_value(params["limit"].clone()).unwrap_or(50);
    let offset: i64 = serde_json::from_value(params["offset"].clone()).unwrap_or(0);

    let atoms = crate::db::queries::search_atoms(
        &state.pool,
        domain_filter.as_deref(),
        type_filter.as_deref(),
        lifecycle_filter.as_deref(),
        limit,
        offset,
    ).await?;

    Ok(json!({ "atoms": atoms }))
}

async fn handle_query_cluster(
    _state: &AppState,
    _params: Option<Value>,
) -> Result<Value> {
    Err(MoteError::Validation("not yet implemented".to_string()))
}

async fn handle_claim_direction(
    state: &AppState,
    params: Option<Value>,
) -> Result<Value> {
    // Auth and rate limiting for mutating methods
    let _verified_agent_id = authenticate_and_rate_limit(state, &params).await?;
    
    // TODO: Implement claim_direction logic
    Err(MoteError::Validation("not yet implemented".to_string()))
}

async fn handle_publish_atoms(
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

    let mut published_atoms = Vec::new();
    
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

        let atom_input = crate::domain::atom::AtomInput {
            atom_type,
            domain: serde_json::from_value(atom_value["domain"].clone())
                .map_err(|_| MoteError::Validation("domain field required".to_string()))?,
            statement: serde_json::from_value(atom_value["statement"].clone())
                .map_err(|_| MoteError::Validation("statement field required".to_string()))?,
            conditions,
            metrics,
            provenance,
            signature: serde_json::from_value(atom_value["signature"].clone())
                .map_err(|_| MoteError::Validation("signature field required".to_string()))?,
        };

        let atom_id = crate::db::queries::publish_atom(&state.pool, &agent_id, atom_input).await?;
        
        // Update graph cache incrementally
        let mut cache = state.graph_cache.write().await;
        cache.add_node(atom_id.clone());
        
        published_atoms.push(atom_id);
    }

    // Increment publish requests accepted counter
    state.metrics.publish_requests_accepted.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    state.metrics.publish_requests_queued.fetch_add(published_atoms.len() as u64, std::sync::atomic::Ordering::Relaxed);

    Ok(json!({ "published_atoms": published_atoms }))
}

async fn handle_retract_atom(
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

async fn handle_get_suggestions(
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

async fn handle_get_field_map(
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
