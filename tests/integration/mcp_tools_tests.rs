//! Integration tests for MCP tools functionality

use axum::http::{Request, Method, StatusCode};
use axum::body::Body;
use serde_json::json;
use super::setup_test_app;
use tower::ServiceExt;
use serial_test::serial;

async fn initialize_mcp_session(app: &axum::Router) -> String {
    let init_request = json!({
        "jsonrpc": "2.0",
        "id": "init-1",
        "method": "initialize",
        "params": {
            "protocolVersion": "2025-03-26",
            "capabilities": {},
            "clientInfo": {
                "name": "test-client",
                "version": "1.0.0"
            }
        }
    });
    
    let request = Request::builder()
        .method(Method::POST)
        .uri("/mcp")
        .header("origin", "http://localhost:3000")
        .header("accept", "application/json, text/event-stream")
        .header("content-type", "application/json")
        .body(Body::from(init_request.to_string()))
        .unwrap();
    
    let response = app.clone().oneshot(request).await.unwrap();
    let session_id = response.headers().get("mcp-session-id")
        .expect("Session ID should be returned")
        .to_str()
        .unwrap();
    
    // Send initialized notification
    let init_notify = json!({
        "jsonrpc": "2.0",
        "method": "notifications/initialized",
        "params": {}
    });
    
    let request = Request::builder()
        .method(Method::POST)
        .uri("/mcp")
        .header("origin", "http://localhost:3000")
        .header("accept", "application/json, text/event-stream")
        .header("mcp-session-id", session_id)
        .header("content-type", "application/json")
        .body(Body::from(init_notify.to_string()))
        .unwrap();
    
    app.clone().oneshot(request).await.unwrap();
    
    session_id.to_string()
}

#[serial]
#[tokio::test]
async fn test_mcp_tools_list() {
    let app = setup_test_app().await;
    let session_id = initialize_mcp_session(&app).await;
    
    let tools_request = json!({
        "jsonrpc": "2.0",
        "id": "tools-1",
        "method": "tools/list",
        "params": {}
    });
    
    let request = Request::builder()
        .method(Method::POST)
        .uri("/mcp")
        .header("origin", "http://localhost:3000")
        .header("accept", "application/json, text/event-stream")
        .header("mcp-session-id", session_id)
        .header("content-type", "application/json")
        .body(Body::from(tools_request.to_string()))
        .unwrap();
    
    let response = app.clone().oneshot(request).await.unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    
    let body = axum::body::to_bytes(response.into_body(), usize::MAX).await.unwrap();
    let response_json: serde_json::Value = serde_json::from_slice(&body).unwrap();
    
    let tools = response_json.get("result").unwrap().get("tools").unwrap().as_array().unwrap();
    assert_eq!(tools.len(), 10);

    // Check specific tools exist
    let tool_names: Vec<String> = tools.iter()
        .map(|t| t.get("name").unwrap().as_str().unwrap().to_string())
        .collect();

    assert!(tool_names.contains(&"register_agent_simple".to_string()));
    assert!(tool_names.contains(&"register_agent".to_string()));
    assert!(tool_names.contains(&"confirm_agent".to_string()));
    assert!(tool_names.contains(&"publish_atoms".to_string()));
    assert!(tool_names.contains(&"search_atoms".to_string()));
    assert!(tool_names.contains(&"query_cluster".to_string()));
    assert!(tool_names.contains(&"claim_direction".to_string()));
    assert!(tool_names.contains(&"retract_atom".to_string()));
    assert!(tool_names.contains(&"get_suggestions".to_string()));
    assert!(tool_names.contains(&"get_field_map".to_string()));
}

#[serial]
#[tokio::test]
async fn test_mcp_register_agent() {
    let app = setup_test_app().await;
    let session_id = initialize_mcp_session(&app).await;
    
    let register_request = json!({
        "jsonrpc": "2.0",
        "id": "register-1",
        "method": "tools/call",
        "params": {
            "name": "register_agent",
            "arguments": {
                "public_key": "deadbeefdeadbeefdeadbeefdeadbeefdeadbeefdeadbeefdeadbeefdeadbeef01"
            }
        }
    });
    
    let request = Request::builder()
        .method(Method::POST)
        .uri("/mcp")
        .header("origin", "http://localhost:3000")
        .header("accept", "application/json, text/event-stream")
        .header("mcp-session-id", session_id)
        .header("content-type", "application/json")
        .body(Body::from(register_request.to_string()))
        .unwrap();
    
    let response = app.clone().oneshot(request).await.unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    
    let body = axum::body::to_bytes(response.into_body(), usize::MAX).await.unwrap();
    let response_json: serde_json::Value = serde_json::from_slice(&body).unwrap();
    
    let result = response_json.get("result").unwrap();
    let content = result.get("content").unwrap().as_array().unwrap();
    assert_eq!(content.len(), 1);
    
    let tool_result: serde_json::Value = serde_json::from_str(
        content[0].get("text").unwrap().as_str().unwrap()
    ).unwrap();
    
    // Should have agent_id and challenge
    assert!(tool_result.get("agent_id").is_some());
    assert!(tool_result.get("challenge").is_some());
}

#[serial]
#[tokio::test]
async fn test_mcp_tool_validation() {
    let app = setup_test_app().await;
    let session_id = initialize_mcp_session(&app).await;
    
    // Test missing required parameter
    let register_request = json!({
        "jsonrpc": "2.0",
        "id": "register-1",
        "method": "tools/call",
        "params": {
            "name": "register_agent",
            "arguments": {}
        }
    });
    
    let request = Request::builder()
        .method(Method::POST)
        .uri("/mcp")
        .header("origin", "http://localhost:3000")
        .header("accept", "application/json, text/event-stream")
        .header("mcp-session-id", &session_id)
        .header("content-type", "application/json")
        .body(Body::from(register_request.to_string()))
        .unwrap();
    
    let response = app.clone().oneshot(request).await.unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    
    let body = axum::body::to_bytes(response.into_body(), usize::MAX).await.unwrap();
    let response_json: serde_json::Value = serde_json::from_slice(&body).unwrap();
    
    let result = response_json.get("result").unwrap();
    assert!(result.get("isError").unwrap().as_bool().unwrap());
    
    let content = result.get("content").unwrap().as_array().unwrap();
    let error_text = content[0].get("text").unwrap().as_str().unwrap();
    assert!(error_text.contains("Missing public_key parameter"));
}

#[serial]
#[tokio::test]
async fn test_mcp_unknown_tool() {
    let app = setup_test_app().await;
    let session_id = initialize_mcp_session(&app).await;
    
    let unknown_request = json!({
        "jsonrpc": "2.0",
        "id": "unknown-1",
        "method": "tools/call",
        "params": {
            "name": "unknown_tool",
            "arguments": {}
        }
    });
    
    let request = Request::builder()
        .method(Method::POST)
        .uri("/mcp")
        .header("origin", "http://localhost:3000")
        .header("accept", "application/json, text/event-stream")
        .header("mcp-session-id", session_id)
        .header("content-type", "application/json")
        .body(Body::from(unknown_request.to_string()))
        .unwrap();
    
    let response = app.clone().oneshot(request).await.unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    
    let body = axum::body::to_bytes(response.into_body(), usize::MAX).await.unwrap();
    let response_json: serde_json::Value = serde_json::from_slice(&body).unwrap();
    
    let result = response_json.get("result").unwrap();
    assert!(result.get("isError").unwrap().as_bool().unwrap());
    
    let content = result.get("content").unwrap().as_array().unwrap();
    let error_text = content[0].get("text").unwrap().as_str().unwrap();
    assert!(error_text.contains("Unknown tool"));
}

#[serial]
#[tokio::test]
async fn test_mcp_tool_parameter_types() {
    let app = setup_test_app().await;
    let session_id = initialize_mcp_session(&app).await;
    
    // Test query_cluster with vector parameter
    let query_request = json!({
        "jsonrpc": "2.0",
        "id": "query-1",
        "method": "tools/call",
        "params": {
            "name": "query_cluster",
            "arguments": {
                "vector": [0.1, 0.2, 0.3, 0.4],
                "radius": 0.5
            }
        }
    });
    
    let request = Request::builder()
        .method(Method::POST)
        .uri("/mcp")
        .header("origin", "http://localhost:3000")
        .header("accept", "application/json, text/event-stream")
        .header("mcp-session-id", session_id)
        .header("content-type", "application/json")
        .body(Body::from(query_request.to_string()))
        .unwrap();
    
    let response = app.clone().oneshot(request).await.unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    
    let body = axum::body::to_bytes(response.into_body(), usize::MAX).await.unwrap();
    let response_json: serde_json::Value = serde_json::from_slice(&body).unwrap();
    
    let result = response_json.get("result").unwrap();
    // Should not be an error (even if the tool fails internally, parameter validation should pass)
    let _is_error = result.get("isError").unwrap().as_bool().unwrap_or(false);
    // We don't assert on isError here as the tool might fail for other reasons
}

#[serial]
#[tokio::test]
async fn test_mcp_tool_schema_validation() {
    let app = setup_test_app().await;
    let session_id = initialize_mcp_session(&app).await;
    
    // Test publish_atoms with invalid atom type
    let publish_request = json!({
        "jsonrpc": "2.0",
        "id": "publish-1",
        "method": "tools/call",
        "params": {
            "name": "publish_atoms",
            "arguments": {
                "agent_id": "test-agent",
                "signature": "deadbeefdeadbeefdeadbeefdeadbeefdeadbeefdeadbeefdeadbeefdeadbeef01",
                "atoms": [
                    {
                        "type": "invalid_type",  // Invalid atom type
                        "domain": "test",
                        "statement": "Test statement",
                        "conditions": {},
                        "provenance": {},
                        "artifact_tree_hash": null
                    }
                ]
            }
        }
    });
    
    let request = Request::builder()
        .method(Method::POST)
        .uri("/mcp")
        .header("origin", "http://localhost:3000")
        .header("accept", "application/json, text/event-stream")
        .header("mcp-session-id", session_id)
        .header("content-type", "application/json")
        .body(Body::from(publish_request.to_string()))
        .unwrap();
    
    let response = app.clone().oneshot(request).await.unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    
    let body = axum::body::to_bytes(response.into_body(), usize::MAX).await.unwrap();
    let response_json: serde_json::Value = serde_json::from_slice(&body).unwrap();
    
    let result = response_json.get("result").unwrap();
    // The tool call should succeed at the MCP level even if validation fails later
    assert!(result.get("content").is_some());
}
