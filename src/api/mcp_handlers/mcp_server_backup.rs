use crate::api::mcp_session::{Capabilities, ClientInfo, SessionStore};
use crate::error::{MoteError, Result};
use crate::state::AppState;
use axum::extract::State;
use axum::http::{HeaderMap, StatusCode};
use axum::response::{IntoResponse, Json, Response};
use serde_json::{json, Value};
use std::sync::Arc;

const HEADER_SESSION_ID: &str = "MCP-Session-Id";
const HEADER_PROTOCOL_VERSION: &str = "MCP-Protocol-Version";
const CURRENT_PROTOCOL_VERSION: &str = "2025-11-25";
const FALLBACK_PROTOCOL_VERSION: &str = "2025-03-26";
const SUPPORTED_PROTOCOL_VERSIONS: [&str; 2] = [CURRENT_PROTOCOL_VERSION, FALLBACK_PROTOCOL_VERSION];

#[derive(serde::Deserialize)]
pub struct InitializeRequest {
    #[serde(rename = "protocolVersion")]
    pub protocol_version: String,
    #[serde(default)]
    pub capabilities: Capabilities,
    #[serde(rename = "clientInfo")]
    pub client_info: ClientInfo,
}

#[derive(serde::Serialize)]
pub struct InitializeResult {
    #[serde(rename = "protocolVersion")]
    pub protocol_version: String,
    pub capabilities: ServerCapabilities,
    #[serde(rename = "serverInfo")]
    pub server_info: ServerInfo,
}

#[derive(serde::Serialize)]
pub struct ServerCapabilities {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tools: Option<ToolsCapability>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub resources: Option<ResourcesCapability>,
}

#[derive(serde::Serialize)]
pub struct ToolsCapability {
    #[serde(rename = "listChanged", default, skip_serializing_if = "Option::is_none")]
    pub list_changed: Option<bool>,
}

#[derive(serde::Serialize)]
pub struct ResourcesCapability {
    #[serde(rename = "listChanged", default, skip_serializing_if = "Option::is_none")]
    pub list_changed: Option<bool>,
}

#[derive(serde::Serialize)]
pub struct ServerInfo {
    pub name: String,
    pub version: String,
}

#[derive(serde::Serialize)]
pub struct ToolCallResult {
    pub content: Vec<Content>,
    #[serde(rename = "isError")]
    pub is_error: bool,
}

#[derive(serde::Serialize)]
pub struct Content {
    #[serde(rename = "type")]
    pub content_type: String,
    #[serde(rename = "text")]
    pub text: String,
}

fn generate_session_id() -> String {
    use rand::{RngCore, SeedableRng};

    let mut hasher = blake3::Hasher::new();
    let mut random_bytes = [0u8; 32];
    rand::rngs::StdRng::from_os_rng().fill_bytes(&mut random_bytes);
    hasher.update(&random_bytes);
    hex::encode(hasher.finalize().as_bytes())
}

fn negotiate_protocol_version(client_protocol_version: &str) -> &str {
    if SUPPORTED_PROTOCOL_VERSIONS.contains(&client_protocol_version) {
        client_protocol_version
    } else {
        CURRENT_PROTOCOL_VERSION
    }
}

fn jsonrpc_error_body(id: Option<Value>, code: i32, message: impl Into<String>) -> Value {
    json!({
        "jsonrpc": "2.0",
        "error": {
            "code": code,
            "message": message.into()
        },
        "id": id
    })
}

fn jsonrpc_success_response(id: Option<Value>, result: Value) -> Response {
    Json(json!({
        "jsonrpc": "2.0",
        "id": id,
        "result": result
    }))
    .into_response()
}

fn jsonrpc_error_response(status: StatusCode, id: Option<Value>, code: i32, message: impl Into<String>) -> Response {
    (status, Json(jsonrpc_error_body(id, code, message))).into_response()
}

fn parse_accept_header(header_value: &str) -> Vec<&str> {
    header_value
        .split(',')
        .map(str::trim)
        .filter(|part| !part.is_empty())
        .collect()
}

fn has_accept_media_type(media_types: &[&str], required: &str) -> bool {
    media_types
        .iter()
        .any(|part| part.eq_ignore_ascii_case(required))
}

fn validate_origin(headers: &HeaderMap, allowed_origins: &[&str]) -> std::result::Result<(), (StatusCode, String)> {
    let Some(origin_header) = headers.get("origin") else {
        return Ok(());
    };

    let origin = origin_header
        .to_str()
        .map_err(|_| (StatusCode::BAD_REQUEST, "Invalid Origin header".to_string()))?;

    if allowed_origins.contains(&origin) {
        Ok(())
    } else {
        Err((StatusCode::FORBIDDEN, format!("Origin not allowed: {origin}")))
    }
}

fn validate_post_accept_header(headers: &HeaderMap) -> std::result::Result<(), (StatusCode, String)> {
    // Accept header is optional for non-browser MCP clients (e.g. supergateway, SDK proxies).
    let Some(accept_header) = headers.get("accept") else {
        return Ok(());
    };

    let accept = accept_header
        .to_str()
        .map_err(|_| (StatusCode::BAD_REQUEST, "Invalid Accept header".to_string()))?;

    // Wildcard always accepted.
    if accept.contains("*/*") {
        return Ok(());
    }

    let media_types = parse_accept_header(accept);
    if has_accept_media_type(&media_types, "application/json")
        || has_accept_media_type(&media_types, "text/event-stream")
    {
        Ok(())
    } else {
        Err((
            StatusCode::BAD_REQUEST,
            "Accept header must include application/json or text/event-stream".to_string(),
        ))
    }
}

fn validate_protocol_version_header(
    headers: &HeaderMap,
    expected_version: Option<&str>,
) -> std::result::Result<(), (StatusCode, String)> {
    let Some(version_header) = headers.get(HEADER_PROTOCOL_VERSION) else {
        return Ok(());
    };

    let protocol_version = version_header
        .to_str()
        .map_err(|_| (StatusCode::BAD_REQUEST, "Invalid MCP-Protocol-Version header".to_string()))?;

    if !SUPPORTED_PROTOCOL_VERSIONS.contains(&protocol_version) {
        return Err((
            StatusCode::BAD_REQUEST,
            format!("Unsupported MCP-Protocol-Version: {protocol_version}"),
        ));
    }

    if let Some(expected) = expected_version {
        if protocol_version != expected {
            return Err((
                StatusCode::BAD_REQUEST,
                format!(
                    "Protocol version mismatch: expected {expected}, got {protocol_version}"
                ),
            ));
        }
    }

    Ok(())
}

fn error_status(error: &MoteError) -> StatusCode {
    match error {
        MoteError::Authentication(_) => StatusCode::UNAUTHORIZED,
        MoteError::NotFound(_) => StatusCode::NOT_FOUND,
        MoteError::RateLimit => StatusCode::TOO_MANY_REQUESTS,
        MoteError::Validation(_) => StatusCode::BAD_REQUEST,
        _ => StatusCode::INTERNAL_SERVER_ERROR,
    }
}

pub async fn handle_mcp_request(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    body: String,
) -> std::result::Result<Response, (StatusCode, String)> {
    let allowed_origins: Vec<&str> = state.config.mcp.allowed_origins.iter().map(String::as_str).collect();
    if let Err((status, msg)) = validate_origin(&headers, &allowed_origins) {
        return Err((status, msg));
    }
    if let Err((status, msg)) = validate_post_accept_header(&headers) {
        return Err((status, msg));
    }

    let request: Value = match serde_json::from_str(&body) {
        Ok(value) => value,
        Err(_) => {
            return Err((
                StatusCode::BAD_REQUEST,
                jsonrpc_error_body(None, -32700, "Parse error").to_string()
            ));
        }
    };

    if request.is_array() {
        return Ok(jsonrpc_error_response(
            StatusCode::BAD_REQUEST,
            None,
            -32600,
            "Batch requests are not supported",
        ));
    }

    if request.get("method").is_none() && request.get("id").is_some() {
        return Ok(StatusCode::ACCEPTED.into_response());
    }

    let request_id = request.get("id").cloned();
    let is_notification = request_id.is_none();

    let method = match request.get("method").and_then(Value::as_str) {
        Some(method) => method,
        None => {
            return Ok(jsonrpc_error_response(
                StatusCode::BAD_REQUEST,
                None,
                -32600,
                "Invalid Request",
            ));
        }
    };

    if method != "initialize" {
        let header_session_id = headers
            .get(HEADER_SESSION_ID)
            .and_then(|value| value.to_str().ok());

        if let Some(session_id) = header_session_id {
            if let Some(session) = state.session_store.get_session(session_id) {
                validate_protocol_version_header(&headers, Some(&session.protocol_version))?;
            } else {
                return Ok(jsonrpc_error_response(
                    StatusCode::NOT_FOUND,
                    None,
                    -32003,
                    "Session not found",
                ));
            }
        } else {
            validate_protocol_version_header(&headers, Some(FALLBACK_PROTOCOL_VERSION))?;
        }
    }

    let result = match method {
        "initialize" => match handle_initialize(&state.session_store, &request).await {
            Ok(response) => return Ok(response),
            Err(error) => Err(error),
        },
        "notifications/initialized" => {
            handle_notifications_initialized(&state.session_store, &headers).await
        }
        "tools/list" => handle_tools_list(&state.session_store, &headers).await,
        "tools/call" => handle_tools_call(&state, &state.session_store, &request, &headers).await,
        "resources/list" => handle_resources_list(&state.session_store, &headers).await,
        "resources/templates/list" => {
            handle_resources_templates_list(&state.session_store, &headers).await
        }
        "resources/read" => {
            handle_resources_read(&state, &state.session_store, &request, &headers).await
        }
        "ping" => handle_ping(&state.session_store, &headers).await,
        _ => {
            let message = format!("Method not found: {method}");
            if is_notification {
                return Ok(jsonrpc_error_response(
                    StatusCode::BAD_REQUEST,
                    None,
                    -32601,
                    message,
                ));
            }
            return Ok(jsonrpc_error_response(StatusCode::OK, request_id, -32601, message));
        }
    };

    match result {
        Ok(result_value) => {
            if is_notification {
                Ok(StatusCode::ACCEPTED.into_response())
            } else {
                Ok(jsonrpc_success_response(request_id, result_value))
            }
        }
        Err(error) => {
            if is_notification {
                Ok(jsonrpc_error_response(
                    error_status(&error),
                    None,
                    error.json_rpc_code(),
                    error.to_string(),
                ))
            } else {
                Ok(jsonrpc_error_response(
                    StatusCode::OK,
                    request_id,
                    error.json_rpc_code(),
                    error.to_string(),
                ))
            }
        }
    }
}

async fn handle_initialize(session_store: &SessionStore, request: &Value) -> Result<Response> {
    let params = request
        .get("params")
        .cloned()
        .ok_or_else(|| MoteError::Validation("Missing params".to_string()))?;

    let init_req: InitializeRequest =
        serde_json::from_value(params).map_err(|_| MoteError::Validation("Invalid initialize params".to_string()))?;

    let negotiated_version = negotiate_protocol_version(&init_req.protocol_version).to_string();
    let session_id = generate_session_id();

    session_store.create_session(
        session_id.clone(),
        init_req.client_info,
        init_req.capabilities,
        negotiated_version.clone(),
    );

    let response_payload = InitializeResult {
        protocol_version: negotiated_version.clone(),
        capabilities: ServerCapabilities {
            tools: Some(ToolsCapability {
                list_changed: Some(false),
            }),
            resources: Some(ResourcesCapability {
                list_changed: Some(false),
            }),
        },
        server_info: ServerInfo {
            name: "mote".to_string(),
            version: env!("CARGO_PKG_VERSION").to_string(),
        },
    };

    let mut response = jsonrpc_success_response(
        request.get("id").cloned(),
        serde_json::to_value(response_payload).map_err(MoteError::Serialization)?,
    );

    response.headers_mut().insert(
        HEADER_SESSION_ID,
        session_id
            .parse::<axum::http::HeaderValue>()
            .map_err(|e| MoteError::Internal(e.to_string()))?,
    );
    response.headers_mut().insert(
        HEADER_PROTOCOL_VERSION,
        negotiated_version
            .parse::<axum::http::HeaderValue>()
            .map_err(|e| MoteError::Internal(e.to_string()))?,
    );

    Ok(response)
}

async fn handle_notifications_initialized(session_store: &SessionStore, headers: &HeaderMap) -> Result<Value> {
    let session_id = headers
        .get(HEADER_SESSION_ID)
        .and_then(|id| id.to_str().ok())
        .ok_or_else(|| MoteError::Validation("Missing MCP-Session-Id header".to_string()))?;

    session_store
        .get_session(session_id)
        .ok_or_else(|| MoteError::NotFound("Session not found".to_string()))?;

    // Idempotent: mark initialized regardless of current state.
    // Some clients retry this notification on reconnect.
    session_store.mark_initialized(session_id);
    Ok(Value::Null)
}

async fn handle_tools_list(session_store: &SessionStore, headers: &HeaderMap) -> Result<Value> {
    validate_session_header(session_store, headers)?;
    let tools = crate::api::mcp_tools::get_all_tools();
    serde_json::to_value(tools).map_err(MoteError::Serialization)
}

async fn handle_tools_call(
    state: &AppState,
    session_store: &SessionStore,
    request: &Value,
    headers: &HeaderMap,
) -> Result<Value> {
    validate_session_header(session_store, headers)?;

    let params = request
        .get("params")
        .ok_or_else(|| MoteError::Validation("Missing params".to_string()))?;

    let tool_name = params
        .get("name")
        .and_then(Value::as_str)
        .ok_or_else(|| MoteError::Validation("Missing name parameter".to_string()))?;

    let arguments = params.get("arguments").cloned().unwrap_or_else(|| json!({}));
    let invocation_result = crate::api::mcp_tools::call_tool(state, tool_name, &arguments).await;

    let tool_result = match invocation_result {
        Ok(result) => ToolCallResult {
            content: vec![Content {
                content_type: "text".to_string(),
                text: serde_json::to_string(&result).map_err(MoteError::Serialization)?,
            }],
            is_error: false,
        },
        Err(error) => ToolCallResult {
            content: vec![Content {
                content_type: "text".to_string(),
                text: error.to_string(),
            }],
            is_error: true,
        },
    };

    serde_json::to_value(tool_result).map_err(MoteError::Serialization)
}

async fn handle_resources_list(session_store: &SessionStore, headers: &HeaderMap) -> Result<Value> {
    validate_session_header(session_store, headers)?;
    let resources = crate::api::mcp_resources::get_concrete_resources();
    serde_json::to_value(resources).map_err(MoteError::Serialization)
}

async fn handle_resources_templates_list(session_store: &SessionStore, headers: &HeaderMap) -> Result<Value> {
    validate_session_header(session_store, headers)?;
    let templates = crate::api::mcp_resources::get_resource_templates();
    serde_json::to_value(templates).map_err(MoteError::Serialization)
}

async fn handle_resources_read(
    state: &AppState,
    session_store: &SessionStore,
    request: &Value,
    headers: &HeaderMap,
) -> Result<Value> {
    validate_session_header(session_store, headers)?;

    let params = request
        .get("params")
        .ok_or_else(|| MoteError::Validation("Missing params".to_string()))?;

    let uri = params
        .get("uri")
        .and_then(Value::as_str)
        .ok_or_else(|| MoteError::Validation("Missing uri parameter".to_string()))?;

    let resource_result = crate::api::mcp_resources::read_resource(state, uri).await?;
    serde_json::to_value(resource_result).map_err(MoteError::Serialization)
}

async fn handle_ping(session_store: &SessionStore, headers: &HeaderMap) -> Result<Value> {
    validate_session_header(session_store, headers)?;
    Ok(json!({}))
}

pub async fn handle_mcp_get() -> impl axum::response::IntoResponse {
    StatusCode::METHOD_NOT_ALLOWED
}

pub async fn handle_mcp_delete(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
) -> std::result::Result<Json<Value>, (StatusCode, String)> {
    let allowed_origins: Vec<&str> = state.config.mcp.allowed_origins.iter().map(String::as_str).collect();
    validate_origin(&headers, &allowed_origins)?;

    let session_id = headers
        .get(HEADER_SESSION_ID)
        .and_then(|id| id.to_str().ok())
        .ok_or_else(|| (StatusCode::BAD_REQUEST, "Missing MCP-Session-Id header".to_string()))?;

    let session = state
        .session_store
        .get_session(session_id)
        .ok_or_else(|| (StatusCode::NOT_FOUND, "Session not found".to_string()))?;

    validate_protocol_version_header(&headers, Some(&session.protocol_version))?;

    if state.session_store.remove_session(session_id) {
        Ok(Json(json!({ "status": "terminated" })))
    } else {
        Err((StatusCode::NOT_FOUND, "Session not found".to_string()))
    }
}

fn validate_session_header(session_store: &SessionStore, headers: &HeaderMap) -> Result<String> {
    let session_id = headers
        .get(HEADER_SESSION_ID)
        .and_then(|id| id.to_str().ok())
        .ok_or_else(|| MoteError::Validation("Missing MCP-Session-Id header".to_string()))?;

    let session = session_store
        .get_session(session_id)
        .ok_or_else(|| MoteError::NotFound("Session not found".to_string()))?;

    if !session.initialized {
        return Err(MoteError::Validation("Session not initialized".to_string()));
    }

    session_store.update_activity(session_id);
    Ok(session_id.to_string())
}
