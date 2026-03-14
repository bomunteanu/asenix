use crate::error::{MoteError, Result};
use crate::state::AppState;
use axum::response::Json;
use serde_json::{json, Value};
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

// Authentication and rate limiting for mutating methods.
// Accepts either token-based auth (api_token field) or Ed25519 signature-based auth
// (signature field). Token-based auth is the recommended path for AI agents.
pub async fn authenticate_and_rate_limit(
    state: &AppState,
    params: &Option<Value>,
) -> Result<String> {
    use crate::crypto::signing::{verify_signature, hex_to_bytes};
    use crate::db::queries::get_agent_by_token;
    
    let params = params.as_ref().ok_or_else(|| {
        MoteError::Validation("Parameters required".to_string())
    })?;

    // Check rate limit first (before expensive crypto operations)
    let client_ip = "127.0.0.1".to_string(); // TODO: Get real IP from request
    state.rate_limiter.validate_ip(&client_ip)?;

    // Try token-based authentication first (recommended for AI agents)
    if let Some(api_token) = params.get("api_token").and_then(|v| v.as_str()) {
        let agent = get_agent_by_token(&state.db_pool, api_token).await?
            .ok_or_else(|| MoteError::Authentication("Invalid API token".to_string()))?;
        
        if !agent.confirmed {
            return Err(MoteError::Authentication("Agent not confirmed".to_string()));
        }
        
        // Apply per-agent rate limiting
        state.rate_limiter.validate_agent(&agent.agent_id)?;
        
        return Ok(agent.agent_id);
    }

    // Fall back to signature-based authentication
    let signature_hex = params.get("signature")
        .and_then(|v| v.as_str())
        .ok_or_else(|| MoteError::Authentication("Signature required".to_string()))?;

    let signature = hex_to_bytes(signature_hex)?;
    let message = serde_json::to_string(params)?;
    let message_hash = crate::crypto::hashing::blake3_hash(&message);

    // Find agent by matching the signature to their public key
    let agent_id = verify_signature_against_all_agents(
        &state.db_pool, 
        &message_hash, 
        &signature
    ).await?;

    // Apply per-agent rate limiting
    state.rate_limiter.validate_agent(&agent_id)?;

    Ok(agent_id)
}

// Helper function to verify signature against all confirmed agents
async fn verify_signature_against_all_agents(
    pool: &sqlx::PgPool,
    message_hash: &[u8; 32],
    signature: &[u8],
) -> Result<String> {
    let rows = sqlx::query("SELECT agent_id, public_key FROM agents WHERE confirmed = true")
        .fetch_all(pool)
        .await?;

    for row in rows {
        let agent_id: String = row.get("agent_id");
        let public_key: Vec<u8> = row.get("public_key");
        
        // Convert signature to array for verification
        if signature.len() == 64 {
            let sig_array: [u8; 64] = signature.try_into()
                .map_err(|_| MoteError::Authentication("Invalid signature format".to_string()))?;
            
            if verify_signature(&public_key, message_hash, &sig_array).is_ok() {
                return Ok(agent_id);
            }
        }
    }

    Err(MoteError::Authentication("Invalid signature".to_string()))
}

pub fn create_jsonrpc_response(result: Option<Value>, error: Option<JsonRpcError>, id: Option<Value>) -> JsonRpcResponse {
    JsonRpcResponse {
        jsonrpc: "2.0".to_string(),
        result,
        error,
        id,
    }
}

pub fn create_jsonrpc_error(code: i32, message: &str, data: Option<Value>, id: Option<Value>) -> JsonRpcResponse {
    create_jsonrpc_response(
        None,
        Some(JsonRpcError {
            code,
            message: message.to_string(),
            data,
        }),
        id,
    )
}
