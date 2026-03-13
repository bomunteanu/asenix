use axum::{
    body::Body,
    extract::Request,
    http::Method,
};
use serde_json::{json, Value};
use tower::ServiceExt;

async fn setup_test_app() -> axum::Router {
    // Create a minimal test app
    axum::Router::new()
        .route("/mcp", axum::routing::post(mote::api::mcp::handle_mcp))
        .with_state(std::sync::Arc::new(mote::state::AppState::new(
            sqlx::PgPool::connect("postgres://mote:mote_password@localhost:5432/mote_test")
                .await
                .expect("Failed to connect to test database"),
            mote::config::Config {
                database: mote::config::DatabaseConfig {
                    url: "postgres://mote:mote_password@localhost:5432/mote_test".to_string(),
                    max_connections: 5,
                },
                hub: mote::config::HubConfig {
                    listen_address: "127.0.0.1:0".to_string(),
                },
                embedding: mote::config::EmbeddingConfig {
                    model_name: "test-model".to_string(),
                    dimension: 384,
                    batch_size: 32,
                },
                rate_limit: mote::config::RateLimitConfig {
                    max_atoms_per_hour: 100,
                },
            },
            tokio::sync::mpsc::channel(100).0,
            tokio::sync::broadcast::channel(100).0,
        ).await))
}

#[tokio::test]
async fn test_mcp_unknown_method() {
    let app = setup_test_app().await;

    let request = Request::builder()
        .method(Method::POST)
        .uri("/mcp")
        .header("content-type", "application/json")
        .body(Body::from(json!({
            "jsonrpc": "2.0",
            "method": "unknown_method",
            "params": {},
            "id": 1
        }).to_string()))
        .unwrap();

    let response = app.clone().oneshot(request).await.unwrap();
    let (_, body) = response.into_parts();

    let response_body: Value = serde_json::from_slice(&axum::body::to_bytes(body, usize::MAX).await.unwrap()).unwrap();

    assert_eq!(response_body["jsonrpc"], "2.0");
    assert_eq!(response_body["id"], 1);
    assert!(response_body["error"].is_object());
    assert_eq!(response_body["error"]["code"], -32601);
    assert_eq!(response_body["error"]["message"], "Method not found: unknown_method");
    assert!(response_body["result"].is_null());
}

#[tokio::test]
async fn test_mcp_missing_jsonrpc_field() {
    let app = setup_test_app().await;

    let request = Request::builder()
        .method(Method::POST)
        .uri("/mcp")
        .header("content-type", "application/json")
        .body(Body::from(json!({
            "method": "register_agent",
            "params": {},
            "id": 1
        }).to_string()))
        .unwrap();

    let response = app.clone().oneshot(request).await.unwrap();
    let (_, body) = response.into_parts();

    let response_body: Value = serde_json::from_slice(&axum::body::to_bytes(body, usize::MAX).await.unwrap()).unwrap();

    assert_eq!(response_body["jsonrpc"], "2.0");
    assert_eq!(response_body["id"], Value::Null);
    assert!(response_body["error"].is_object());
    assert_eq!(response_body["error"]["code"], -32600);
    assert_eq!(response_body["error"]["message"], "Invalid Request");
    assert!(response_body["result"].is_null());
}

#[tokio::test]
async fn test_mcp_malformed_json() {
    let app = setup_test_app().await;

    let request = Request::builder()
        .method(Method::POST)
        .uri("/mcp")
        .header("content-type", "application/json")
        .body(Body::from("{ invalid json".to_string()))
        .unwrap();

    let response = app.clone().oneshot(request).await.unwrap();
    let (_, body) = response.into_parts();

    let response_body: Value = serde_json::from_slice(&axum::body::to_bytes(body, usize::MAX).await.unwrap()).unwrap();

    assert_eq!(response_body["jsonrpc"], "2.0");
    assert_eq!(response_body["id"], Value::Null);
    assert!(response_body["error"].is_object());
    assert_eq!(response_body["error"]["code"], -32700);
    assert_eq!(response_body["error"]["message"], "Parse error");
    assert!(response_body["result"].is_null());
}

#[tokio::test]
async fn test_mcp_batch_request_not_supported() {
    let app = setup_test_app().await;

    let request = Request::builder()
        .method(Method::POST)
        .uri("/mcp")
        .header("content-type", "application/json")
        .body(Body::from(json!([
            {
                "jsonrpc": "2.0",
                "method": "register_agent",
                "params": {},
                "id": 1
            }
        ]).to_string()))
        .unwrap();

    let response = app.clone().oneshot(request).await.unwrap();
    let (_, body) = response.into_parts();

    let response_body: Value = serde_json::from_slice(&axum::body::to_bytes(body, usize::MAX).await.unwrap()).unwrap();

    assert_eq!(response_body["jsonrpc"], "2.0");
    assert_eq!(response_body["id"], Value::Null);
    assert!(response_body["error"].is_object());
    assert_eq!(response_body["error"]["code"], -32700);
    assert_eq!(response_body["error"]["message"], "Batch requests not supported");
    assert!(response_body["result"].is_null());
}

#[tokio::test]
async fn test_mcp_register_agent_success() {
    let app = setup_test_app().await;

    let request = Request::builder()
        .method(Method::POST)
        .uri("/mcp")
        .header("content-type", "application/json")
        .body(Body::from(json!({
            "jsonrpc": "2.0",
            "method": "register_agent",
            "params": {
                "public_key": "abc123def456"
            },
            "id": "test-1"
        }).to_string()))
        .unwrap();

    let response = app.clone().oneshot(request).await.unwrap();
    let (_, body) = response.into_parts();

    let response_body: Value = serde_json::from_slice(&axum::body::to_bytes(body, usize::MAX).await.unwrap()).unwrap();

    assert_eq!(response_body["jsonrpc"], "2.0");
    assert_eq!(response_body["id"], "test-1");
    assert!(response_body["error"].is_null());
    assert!(response_body["result"].is_object());
    assert!(response_body["result"]["agent_id"].is_string());
    assert!(response_body["result"]["challenge"].is_string());
}
