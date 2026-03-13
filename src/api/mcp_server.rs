use crate::error::{MoteError, Result};
use crate::state::AppState;
use crate::api::mcp_session::{SessionStore, ClientInfo, Capabilities};
use axum::extract::{State};
use axum::http::{HeaderMap};
use axum::response::Json;
use serde_json::{json, Value};
use std::sync::Arc;
use blake3::Hasher;
use hex;

/// MCP request/response structures for initialize
#[derive(serde::Deserialize)]
pub struct InitializeRequest {
    pub protocol_version: String,
    pub capabilities: Option<Value>,
    pub client_info: ClientInfo,
}

#[derive(serde::Serialize)]
pub struct InitializeResult {
    pub protocol_version: String,
    pub capabilities: ServerCapabilities,
    pub server_info: ServerInfo,
}

#[derive(serde::Serialize)]
pub struct ServerCapabilities {
    #[serde(rename = "tools")]
    pub tools: Option<ToolsCapability>,
    #[serde(rename = "resources")]
    pub resources: Option<ResourcesCapability>,
}

#[derive(serde::Serialize)]
pub struct ToolsCapability {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub list_changed: Option<bool>,
}

#[derive(serde::Serialize)]
pub struct ResourcesCapability {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub list_changed: Option<bool>,
}

#[derive(serde::Serialize)]
pub struct ServerInfo {
    pub name: String,
    pub version: String,
}

/// MCP tool call request
#[derive(serde::Deserialize)]
pub struct ToolCallRequest {
    pub name: String,
    pub arguments: Value,
}

/// MCP tool call result
#[derive(serde::Serialize)]
pub struct ToolCallResult {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub content: Option<Vec<Content>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub is_error: Option<bool>,
}

#[derive(serde::Serialize)]
pub struct Content {
    #[serde(rename = "type")]
    pub content_type: String,
    #[serde(rename = "text")]
    pub text: String,
}

/// Generate a BLAKE3 session ID
fn generate_session_id() -> String {
    use blake3::Hasher;
    use rand::RngCore;
    
    let mut hasher = Hasher::new();
    let mut rng = rand::rng();
    let mut random_bytes = [0u8; 32];
    rng.fill_bytes(&mut random_bytes);
    hasher.update(&random_bytes);
    
    hex::encode(hasher.finalize().as_bytes())
}

/// Validate Origin header against allowlist
fn validate_origin(headers: &HeaderMap, allowed_origins: &[&str]) -> Result<()> {
    if let Some(origin_value) = headers.get("origin") {
        let origin = origin_value.to_str().map_err(|_| MoteError::Validation("Invalid Origin header".to_string()))?;
        
        if !allowed_origins.contains(&origin) {
            return Err(MoteError::Validation(format!(
                "Origin not allowed: {}. Allowed: {:?}",
                origin,
                allowed_origins
            )));
        }
    } else {
        return Err(MoteError::Validation("Missing Origin header".to_string()));
    }
    
    Ok(())
}

/// Validate Accept header for MCP
fn validate_accept_header(headers: &HeaderMap) -> Result<()> {
    if let Some(accept_value) = headers.get("accept") {
        let accept = accept_value.to_str().map_err(|_| MoteError::Validation("Invalid Accept header".to_string()))?;
        
        let required_types = ["application/json", "text/event-stream"];
        
        if !required_types.iter().any(|required_type| {
            accept.contains(required_type)
        }) {
            return Err(MoteError::Validation(format!(
                "Accept header must include both application/json and text/event-stream. Got: {}",
                accept
            )));
        }
    } else {
        return Err(MoteError::Validation("Missing Accept header".to_string()));
    }
    
    Ok(())
}

/// Handle POST /mcp requests
pub async fn handle_mcp_request(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    body: String,
) -> std::result::Result<Json<serde_json::Value>, (axum::http::StatusCode, String)> {
    let session_store = SessionStore::new();
    
    // Parse JSON body
    let request_value: serde_json::Value = serde_json::from_str(&body)
        .map_err(|_| (axum::http::StatusCode::BAD_REQUEST, "Invalid JSON".to_string()))?;
    
    // Check if this is a batch request (not supported)
    if request_value.is_array() {
        return Ok(Json(json!({
            "jsonrpc": "2.0",
            "error": {
                "code": -32600,
                "message": "Batch requests not supported",
                "data": {
                    "request_id": generate_session_id(),
                    "timestamp": chrono::Utc::now().to_rfc3339()
                }
            }
        })));
    }
    
    // Parse single request
    let request: serde_json::Value = serde_json::from_value(request_value)
        .map_err(|_| (axum::http::StatusCode::BAD_REQUEST, "Invalid request".to_string()))?;
    
    // Extract request ID for logging
    let request_id = request
        .get("id")
        .and_then(|id| id.as_str())
        .unwrap_or("unknown");
    
    // Route by method
    let result = match request
        .get("method")
        .and_then(|m| m.as_str())
        .unwrap_or("unknown") {
        "initialize" => handle_initialize(&state, &session_store, &request, &headers).await,
        "notifications/initialized" => handle_notifications_initialized(&session_store, &request, &headers).await,
        "tools/list" => handle_tools_list(&session_store, &request, &headers).await,
        "tools/call" => handle_tools_call(&state, &session_store, &request, &headers).await,
        "resources/list" => handle_resources_list(&session_store, &request, &headers).await,
        "resources/templates/list" => handle_resources_templates_list(&session_store, &request, &headers).await,
        "resources/read" => handle_resources_read(&state, &session_store, &request, &headers).await,
        "ping" => handle_ping(&headers).await,
        _ => handle_unknown_method(&request, &headers).await,
    };
    
    match result {
        Ok(response_value) => Ok(Json(response_value)),
        Err((status, error_msg)) => Ok(Json(json!({
            "jsonrpc": "2.0",
            "error": {
                "code": -32600,
                "message": error_msg,
                "data": {
                    "request_id": request_id,
                    "timestamp": chrono::Utc::now().to_rfc3339()
                }
            },
            "id": request.get("id")
        }))),
    }
}

/// Handle initialize request
async fn handle_initialize(
    state: &AppState,
    session_store: &SessionStore,
    request: &serde_json::Value,
    headers: &HeaderMap,
) -> std::result::Result<serde_json::Value, (axum::http::StatusCode, String)> {
    // Validate Origin and Accept headers for initialize (no session required)
    validate_origin(headers, &state.config.mcp.allowed_origins.iter().map(|s| s.as_str()).collect::<Vec<_>>())
        .map_err(|e| (axum::http::StatusCode::BAD_REQUEST, e.to_string()))?;
    validate_accept_header(headers)
        .map_err(|e| (axum::http::StatusCode::BAD_REQUEST, e.to_string()))?;
    
    // Parse initialize request
    let init_req: InitializeRequest = serde_json::from_value(request.get("params").cloned().unwrap_or_default())
        .map_err(|_| (axum::http::StatusCode::BAD_REQUEST, "Invalid initialize params".to_string()))?;
    
    // Generate session ID
    let session_id = generate_session_id();
    
    // Create session
    let client_info = ClientInfo {
        name: init_req.client_info.name,
        version: init_req.client_info.version,
    };
    
    let capabilities = Capabilities {
        tools: Some(crate::api::mcp_session::ToolsCapability { list_changed: None }),
        resources: Some(crate::api::mcp_session::ResourcesCapability { list_changed: None }),
    };
    
    session_store.create_session(
        session_id.clone(),
        client_info,
        capabilities,
        "2025-03-26".to_string(),
    );
    
    let response = InitializeResult {
        protocol_version: "2025-03-26".to_string(),
        capabilities: ServerCapabilities {
            tools: Some(ToolsCapability { list_changed: None }),
            resources: Some(ResourcesCapability { list_changed: None }),
        },
        server_info: ServerInfo {
            name: "mote".to_string(),
            version: env!("CARGO_PKG_VERSION").to_string(),
        },
    };
    
    let response_value = serde_json::to_value(response)
        .map_err(|e| (axum::http::StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;
    Ok(response_value)
}

/// Handle notifications/initialized
async fn handle_notifications_initialized(
    session_store: &SessionStore,
    request: &serde_json::Value,
    headers: &HeaderMap,
) -> std::result::Result<serde_json::Value, (axum::http::StatusCode, String)> {
    // Extract session ID from headers
    let session_id = headers
        .get("mcp-session-id")
        .and_then(|id| id.to_str().ok())
        .ok_or_else(|| (axum::http::StatusCode::BAD_REQUEST, "Missing Mcp-Session-Id header".to_string()))?;
    
    // Validate session exists and is not initialized
    let session = session_store.get_session(&session_id)
        .ok_or_else(|| (axum::http::StatusCode::BAD_REQUEST, "Invalid session".to_string()))?;
    
    if session.initialized {
        return Err((axum::http::StatusCode::BAD_REQUEST, "Session already initialized".to_string()));
    }
    
    // Mark session as initialized
    session_store.mark_initialized(&session_id);
    
    // Return 202 Accepted with empty body
    Ok(serde_json::Value::Null)
}

/// Handle tools/list
async fn handle_tools_list(
    session_store: &SessionStore,
    request: &serde_json::Value,
    headers: &HeaderMap,
) -> std::result::Result<serde_json::Value, (axum::http::StatusCode, String)> {
    // Validate session
    let session_id = validate_session_header(session_store, headers)?;
    let session = session_store.get_session(&session_id)
        .ok_or_else(|| (axum::http::StatusCode::BAD_REQUEST, "Session not found".to_string()))?;
    
    if !session.initialized {
        return Err((axum::http::StatusCode::BAD_REQUEST, "Session not initialized".to_string()));
    }
    
    // Update activity
    session_store.update_activity(&session_id);
    
    let tools = crate::api::mcp_tools::get_all_tools();
    let response_value = serde_json::to_value(tools)
        .map_err(|e| (axum::http::StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;
    Ok(response_value)
}

/// Handle tools/call
async fn handle_tools_call(
    state: &AppState,
    session_store: &SessionStore,
    request: &serde_json::Value,
    headers: &HeaderMap,
) -> std::result::Result<serde_json::Value, (axum::http::StatusCode, String)> {
    // Validate session
    let session_id = validate_session_header(session_store, headers)?;
    let session = session_store.get_session(&session_id)
        .ok_or_else(|| (axum::http::StatusCode::BAD_REQUEST, "Session not found".to_string()))?;
    
    if !session.initialized {
        return Err((axum::http::StatusCode::BAD_REQUEST, "Session not initialized".to_string()));
    }
    
    // Update activity
    session_store.update_activity(&session_id);
    
    // Dispatch to tool handler
    let tool_name = request
        .get("name")
        .and_then(|n| n.as_str())
        .ok_or_else(|| (axum::http::StatusCode::BAD_REQUEST, "Missing name parameter".to_string()))?;
    
    let arguments = request
        .get("arguments")
        .cloned()
        .unwrap_or_default();
    
    let result = crate::api::mcp_tools::call_tool(state, tool_name, &arguments).await;
    
    match result {
        Ok(result) => {
            let tool_result = ToolCallResult {
                content: Some(vec![Content {
                    content_type: "text".to_string(),
                    text: serde_json::to_string(&result).unwrap(),
                }]),
                is_error: Some(false),
            };
            let response_value = serde_json::to_value(tool_result)
                .map_err(|e| (axum::http::StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;
            Ok(response_value)
        },
        Err(error) => {
            let error_response = json!({
                "error": {
                    "code": -32603,
                    "message": error.to_string()
                }
            });
            let response_value = serde_json::to_value(error_response)
                .map_err(|e| (axum::http::StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;
            Ok(response_value)
        }
    }
}

/// Handle resources/list
async fn handle_resources_list(
    session_store: &SessionStore,
    request: &serde_json::Value,
    headers: &HeaderMap,
) -> std::result::Result<serde_json::Value, (axum::http::StatusCode, String)> {
    // Validate session
    let session_id = validate_session_header(session_store, headers)?;
    let session = session_store.get_session(&session_id)
        .ok_or_else(|| (axum::http::StatusCode::BAD_REQUEST, "Session not found".to_string()))?;
    
    if !session.initialized {
        return Err((axum::http::StatusCode::BAD_REQUEST, "Session not initialized".to_string()));
    }
    
    // Update activity
    session_store.update_activity(&session_id);
    
    let resources = crate::api::mcp_resources::get_concrete_resources();
    let response_value = serde_json::to_value(resources)
        .map_err(|e| (axum::http::StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;
    Ok(response_value)
}

/// Handle resources/templates/list
async fn handle_resources_templates_list(
    session_store: &SessionStore,
    request: &serde_json::Value,
    headers: &HeaderMap,
) -> std::result::Result<serde_json::Value, (axum::http::StatusCode, String)> {
    // Validate session
    let session_id = validate_session_header(session_store, headers)?;
    let session = session_store.get_session(&session_id)
        .ok_or_else(|| (axum::http::StatusCode::BAD_REQUEST, "Session not found".to_string()))?;
    
    if !session.initialized {
        return Err((axum::http::StatusCode::BAD_REQUEST, "Session not initialized".to_string()));
    }
    
    // Update activity
    session_store.update_activity(&session_id);
    
    let templates = crate::api::mcp_resources::get_resource_templates();
    let response_value = serde_json::to_value(templates)
        .map_err(|e| (axum::http::StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;
    Ok(response_value)
}

/// Handle resources/read
async fn handle_resources_read(
    state: &AppState,
    session_store: &SessionStore,
    request: &serde_json::Value,
    headers: &HeaderMap,
) -> std::result::Result<serde_json::Value, (axum::http::StatusCode, String)> {
    // Validate session
    let session_id = validate_session_header(session_store, headers)?;
    let session = session_store.get_session(&session_id)
        .ok_or_else(|| (axum::http::StatusCode::BAD_REQUEST, "Session not found".to_string()))?;
    
    if !session.initialized {
        return Err((axum::http::StatusCode::BAD_REQUEST, "Session not initialized".to_string()));
    }
    
    // Update activity
    session_store.update_activity(&session_id);
    
    // Extract URI from request
    let uri = request
        .get("uri")
        .and_then(|v| v.as_str())
        .ok_or_else(|| (axum::http::StatusCode::BAD_REQUEST, "Missing uri parameter".to_string()))?;
    
    // Dispatch to resource handler
    let result = crate::api::mcp_resources::read_resource(state, uri).await;
    
    match result {
        Ok(resource_result) => {
            let response_value = serde_json::to_value(resource_result)
                .map_err(|e| (axum::http::StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;
            Ok(response_value)
        },
        Err(error) => {
            let error_response = json!({
                "error": {
                    "code": -32603,
                    "message": error.to_string()
                }
            });
            let response_value = serde_json::to_value(error_response)
                .map_err(|e| (axum::http::StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;
            Ok(response_value)
        }
    }
}

/// Handle GET /mcp requests (returns 405 Method Not Allowed)
pub async fn handle_mcp_get() -> std::result::Result<Json<serde_json::Value>, (axum::http::StatusCode, String)> {
    Err((axum::http::StatusCode::METHOD_NOT_ALLOWED, "GET method not allowed on /mcp endpoint".to_string()))
}

/// Handle DELETE /mcp requests (session termination)
pub async fn handle_mcp_delete(
    State(_state): State<Arc<AppState>>,
    headers: HeaderMap,
) -> std::result::Result<Json<serde_json::Value>, (axum::http::StatusCode, String)> {
    let session_store = SessionStore::new();
    
    // Extract session ID from headers
    let session_id = headers
        .get("mcp-session-id")
        .and_then(|id| id.to_str().ok())
        .ok_or_else(|| (axum::http::StatusCode::BAD_REQUEST, "Missing Mcp-Session-Id header".to_string()))?;
    
    // Validate session exists
    let _session = session_store.get_session(&session_id)
        .ok_or_else(|| (axum::http::StatusCode::BAD_REQUEST, "Session not found".to_string()))?;
    
    // Remove session
    if session_store.remove_session(&session_id) {
        Ok(Json(json!({ "status": "terminated" })))
    } else {
        Err((axum::http::StatusCode::NOT_FOUND, "Session not found".to_string()))
    }
}

/// Handle ping
async fn handle_ping(
    headers: &HeaderMap,
) -> std::result::Result<serde_json::Value, (axum::http::StatusCode, String)> {
    Ok(json!({}))
}

/// Handle unknown method
async fn handle_unknown_method(
    request: &serde_json::Value,
    headers: &HeaderMap,
) -> std::result::Result<serde_json::Value, (axum::http::StatusCode, String)> {
    let request_id = request
        .get("id")
        .and_then(|id| id.as_str())
        .unwrap_or("unknown");
    
    Ok(json!({
        "jsonrpc": "2.0",
        "error": {
            "code": -32601,
            "message": format!("Method not found: {}", 
                request.get("method").and_then(|m| m.as_str()).unwrap_or("unknown"))
        },
        "id": request.get("id")
    }))
}

/// Validate session header and return session
fn validate_session_header(
    session_store: &SessionStore,
    headers: &HeaderMap,
) -> std::result::Result<String, (axum::http::StatusCode, String)> {
    let session_id: String = headers
        .get("mcp-session-id")
        .and_then(|id| id.to_str().ok())
        .ok_or_else(|| (axum::http::StatusCode::BAD_REQUEST, "Missing Mcp-Session-Id header".to_string()))?
        .to_string();

    let session = session_store.get_session(&session_id)
        .ok_or_else(|| (axum::http::StatusCode::BAD_REQUEST, "Session not found".to_string()))?;
    
    if !session.initialized {
        return Err((axum::http::StatusCode::BAD_REQUEST, "Session not initialized".to_string()));
    }

    session_store.update_activity(&session_id);
    Ok(session_id)
}

/// Extension trait for error responses
trait IntoErrorResponse {
    fn into_error_response(self, request_id: &str) -> serde_json::Value;
}

impl IntoErrorResponse for MoteError {
    fn into_error_response(self, request_id: &str) -> serde_json::Value {
        json!({
            "jsonrpc": "2.0",
            "error": {
                "code": self.json_rpc_code(),
                "message": self.to_string(),
                "data": {
                    "request_id": request_id,
                    "timestamp": chrono::Utc::now().to_rfc3339()
                }
            },
            "id": Some(request_id)
        })
    }
}
