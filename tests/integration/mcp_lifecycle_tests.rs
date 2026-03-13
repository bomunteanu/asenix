//! Integration tests for MCP lifecycle and tools

use axum::http::{Request, Method, StatusCode};
use axum::body::Body;
use serde_json::json;
use tower::ServiceExt;

use super::setup_test_app;
use super::initialize_session;
use serial_test::serial;

#[serial]
#[tokio::test]
async fn test_mcp_initialize_lifecycle() {
    let app = setup_test_app().await;
    
    // Step 1: Initialize MCP session
    let init_request = json!({
        "jsonrpc": "2.0",
        "id": "init-1",
        "method": "initialize",
        "params": {
            "protocolVersion": "2025-03-26",
            "capabilities": {
                "tools": {},
                "resources": {}
            },
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
    assert_eq!(response.status(), StatusCode::OK);
    
    // Extract session ID from response headers
    let session_id = response.headers().get("mcp-session-id")
        .expect("Session ID should be returned")
        .to_str()
        .unwrap();
    
    // Step 2: Send initialized notification
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
    
    let response = app.clone().oneshot(request).await.unwrap();
    assert_eq!(response.status(), StatusCode::ACCEPTED);
    
    // Step 3: Test tools/list after initialization
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
    assert_eq!(tools.len(), 10); // register_agent_simple + 9 original tools
}

#[serial]
#[tokio::test]
async fn test_mcp_session_validation() {
    let app = setup_test_app().await;
    
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
    
    // Test invalid Origin header → 403 Forbidden
    let request = Request::builder()
        .method(Method::POST)
        .uri("/mcp")
        .header("origin", "http://malicious.com")
        .header("accept", "application/json, text/event-stream")
        .header("content-type", "application/json")
        .body(Body::from(init_request.to_string()))
        .unwrap();
    
    let response = app.clone().oneshot(request).await.unwrap();
    assert_eq!(response.status(), StatusCode::FORBIDDEN);
    
    // Test missing Accept header → now allowed (non-browser MCP clients often omit it)
    let request = Request::builder()
        .method(Method::POST)
        .uri("/mcp")
        .header("origin", "http://localhost:3000")
        .header("content-type", "application/json")
        .body(Body::from(init_request.to_string()))
        .unwrap();

    let response = app.clone().oneshot(request).await.unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    
    // Test no Origin (optional per MCP spec) → should proceed normally
    let request = Request::builder()
        .method(Method::POST)
        .uri("/mcp")
        .header("accept", "application/json, text/event-stream")
        .header("content-type", "application/json")
        .body(Body::from(init_request.to_string()))
        .unwrap();
    
    let response = app.clone().oneshot(request).await.unwrap();
    assert_eq!(response.status(), StatusCode::OK);
}

#[serial]
#[tokio::test]
async fn test_mcp_session_termination() {
    let app = setup_test_app().await;
    
    // Initialize session first (with camelCase fields)
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
    assert_eq!(response.status(), StatusCode::OK);
    let session_id = response.headers().get("mcp-session-id")
        .expect("Session ID should be returned")
        .to_str()
        .unwrap()
        .to_string();
    
    // Terminate session via DELETE
    let request = Request::builder()
        .method(Method::DELETE)
        .uri("/mcp")
        .header("origin", "http://localhost:3000")
        .header("mcp-session-id", &session_id)
        .body(Body::empty())
        .unwrap();
    
    let response = app.clone().oneshot(request).await.unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    
    // Try to use terminated session → 404 Not Found (session gone)
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
        .header("mcp-session-id", &session_id)
        .header("content-type", "application/json")
        .body(Body::from(tools_request.to_string()))
        .unwrap();
    
    let response = app.clone().oneshot(request).await.unwrap();
    assert_eq!(response.status(), StatusCode::NOT_FOUND);
}

#[serial]
#[tokio::test]
async fn test_mcp_uninitialized_session() {
    let app = setup_test_app().await;
    
    // Initialize session but don't send initialized notification
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
    
    // Try to use tools before initialized notification
    // Server returns HTTP 200 with JSON-RPC error for requests with an id
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
    
    // Should contain a JSON-RPC error about session not initialized
    let error = response_json.get("error").expect("Should have error field");
    assert!(error.get("code").is_some());
    assert!(error.get("message").unwrap().as_str().unwrap().contains("not initialized"));
}

#[serial]
#[tokio::test]
async fn test_mcp_ping() {
    let app = setup_test_app().await;
    let session_id = initialize_session(&app).await;
    
    // Test ping
    let ping_request = json!({
        "jsonrpc": "2.0",
        "id": "ping-1",
        "method": "ping",
        "params": {}
    });
    
    let request = Request::builder()
        .method(Method::POST)
        .uri("/mcp")
        .header("origin", "http://localhost:3000")
        .header("accept", "application/json, text/event-stream")
        .header("mcp-session-id", &session_id)
        .header("content-type", "application/json")
        .body(Body::from(ping_request.to_string()))
        .unwrap();
    
    let response = app.clone().oneshot(request).await.unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    
    let body = axum::body::to_bytes(response.into_body(), usize::MAX).await.unwrap();
    let response_json: serde_json::Value = serde_json::from_slice(&body).unwrap();
    
    // Ping should return empty result
    assert!(response_json.get("result").unwrap().as_object().unwrap().is_empty());
}

#[serial]
#[tokio::test]
async fn test_mcp_unknown_method() {
    let app = setup_test_app().await;
    let session_id = initialize_session(&app).await;
    
    // Test unknown method
    let unknown_request = json!({
        "jsonrpc": "2.0",
        "id": "unknown-1",
        "method": "unknown_method",
        "params": {}
    });
    
    let request = Request::builder()
        .method(Method::POST)
        .uri("/mcp")
        .header("origin", "http://localhost:3000")
        .header("accept", "application/json, text/event-stream")
        .header("mcp-session-id", &session_id)
        .header("content-type", "application/json")
        .body(Body::from(unknown_request.to_string()))
        .unwrap();
    
    let response = app.clone().oneshot(request).await.unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    
    let body = axum::body::to_bytes(response.into_body(), usize::MAX).await.unwrap();
    let response_json: serde_json::Value = serde_json::from_slice(&body).unwrap();
    
    // Should return method not found error
    let error = response_json.get("error").unwrap();
    assert_eq!(error.get("code").unwrap().as_i64().unwrap(), -32601);
    assert!(error.get("message").unwrap().as_str().unwrap().contains("Method not found"));
}
