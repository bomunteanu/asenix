use crate::api::mcp::{handle_mcp, JsonRpcRequest};
use crate::state::{AppState, SseEvent};
use crate::config::Config;
use axum::extract::State;
use serde_json::json;
use sqlx::PgPool;
use std::sync::Arc;
use tokio::sync::{broadcast, mpsc};

async fn setup_test_state() -> Arc<AppState> {
    let config = Config::default();
    let pool = sqlx::PgPool::connect("postgresql://test:test@localhost/test_asenix")
        .await
        .expect("Failed to connect to test database");
    
    let (embedding_tx, _) = tokio::sync::mpsc::channel(100);
    let (sse_tx, _) = tokio::sync::broadcast::channel(100);
    
    let state = AppState::new(pool, Arc::new(config), embedding_tx, sse_tx)
        .await
        .expect("Failed to create test state");
    
    Arc::new(state)
}

#[tokio::test]
async fn test_rate_limiting_enforcement() {
    let state = setup_test_state().await;
    
    // Create a test agent
    let agent_id = "test-agent-1";
    sqlx::query("INSERT INTO agents (agent_id, public_key, confirmed) VALUES ($1, $2, TRUE)")
        .bind(agent_id)
        .bind("test-public-key")
        .execute(&state.pool)
        .await
        .expect("Failed to insert test agent");
    
    // Create publish request with valid signature (mocked for testing)
    let request = JsonRpcRequest {
        jsonrpc: "2.0".to_string(),
        method: "publish_atoms".to_string(),
        params: Some(json!({
            "agent_id": agent_id,
            "signature": "test-signature",
            "atoms": [{
                "atom_type": "hypothesis",
                "domain": "test-domain",
                "statement": "Test hypothesis",
                "conditions": {},
                "provenance": {},
                "signature": "test-atom-signature"
            }]
        })),
        id: Some(json!(1)),
    };
    
    let request_body = serde_json::to_string(&request).unwrap();
    
    // Make multiple requests to test rate limiting
    for i in 0..100 {
        let response = handle_mcp(State(state.clone()), request_body.clone()).await;
        
        if i < 50 { // Assuming rate limit is 50 per hour
            assert!(response.is_ok(), "Request {} should succeed", i);
        } else {
            // After rate limit is hit, should get rate limit error
            let result = response.unwrap();
            assert!(result.error.is_some(), "Request {} should be rate limited", i);
            assert_eq!(result.error.unwrap().code, -32002, "Should return rate limit error code");
        }
    }
    
    // Clean up
    sqlx::query("DELETE FROM agents WHERE agent_id = $1")
        .bind(agent_id)
        .execute(&state.pool)
        .await
        .expect("Failed to clean up test agent");
}

#[tokio::test]
async fn test_authentication_required_for_mutating_operations() {
    let state = setup_test_state().await;
    
    // Test publish_atoms without authentication
    let request = JsonRpcRequest {
        jsonrpc: "2.0".to_string(),
        method: "publish_atoms".to_string(),
        params: Some(json!({
            "atoms": [{
                "atom_type": "hypothesis",
                "domain": "test-domain",
                "statement": "Test hypothesis"
            }]
        })),
        id: Some(json!(1)),
    };
    
    let request_body = serde_json::to_string(&request).unwrap();
    let response = handle_mcp(State(state), request_body).await;
    
    assert!(response.is_ok(), "Should return response");
    let result = response.unwrap();
    assert!(result.error.is_some(), "Should return authentication error");
    assert_eq!(result.error.unwrap().code, -32001, "Should return authentication error code");
}

#[tokio::test]
async fn test_read_operations_no_auth_required() {
    let state = setup_test_state().await;
    
    // Test search_atoms (read operation) without authentication
    let request = JsonRpcRequest {
        jsonrpc: "2.0".to_string(),
        method: "search_atoms".to_string(),
        params: Some(json!({
            "domain": "test-domain",
            "limit": 10
        })),
        id: Some(json!(1)),
    };
    
    let request_body = serde_json::to_string(&request).unwrap();
    let response = handle_mcp(State(state), request_body).await;
    
    // Read operations should not require authentication
    assert!(response.is_ok(), "Read operation should succeed without auth");
    let result = response.unwrap();
    assert!(result.error.is_none(), "Should not return authentication error for read operations");
}

#[tokio::test]
async fn test_error_responses_include_request_id() {
    let state = setup_test_state().await;
    
    // Test invalid JSON-RPC version
    let request = JsonRpcRequest {
        jsonrpc: "1.0".to_string(), // Invalid version
        method: "search_atoms".to_string(),
        params: None,
        id: Some(json!(1)),
    };
    
    let request_body = serde_json::to_string(&request).unwrap();
    let response = handle_mcp(State(state), request_body).await;
    
    assert!(response.is_ok(), "Should return error response");
    let result = response.unwrap();
    assert!(result.error.is_some(), "Should return error");
    
    let error = result.error.unwrap();
    assert!(error.data.is_some(), "Error should include data");
    
    let data = error.data.unwrap();
    assert!(data.get("request_id").is_some(), "Error should include request_id");
    assert!(data.get("timestamp").is_some(), "Error should include timestamp");
    assert!(data.get("error_type").is_some(), "Error should include error_type");
}

#[tokio::test]
async fn test_signature_verification() {
    let state = setup_test_state().await;
    
    // Create a test agent
    let agent_id = "test-agent-signature";
    sqlx::query("INSERT INTO agents (agent_id, public_key, confirmed) VALUES ($1, $2, TRUE)")
        .bind(agent_id)
        .bind("test-public-key")
        .execute(&state.pool)
        .await
        .expect("Failed to insert test agent");
    
    // Test with invalid signature
    let request = JsonRpcRequest {
        jsonrpc: "2.0".to_string(),
        method: "publish_atoms".to_string(),
        params: Some(json!({
            "agent_id": agent_id,
            "signature": "invalid-signature",
            "atoms": [{
                "atom_type": "hypothesis",
                "domain": "test-domain",
                "statement": "Test hypothesis",
                "conditions": {},
                "provenance": {},
                "signature": "test-atom-signature"
            }]
        })),
        id: Some(json!(1)),
    };
    
    let request_body = serde_json::to_string(&request).unwrap();
    let response = handle_mcp(State(state), request_body).await;
    
    assert!(response.is_ok(), "Should return response");
    let result = response.unwrap();
    assert!(result.error.is_some(), "Should return signature verification error");
    assert_eq!(result.error.unwrap().code, -32001, "Should return authentication error code");
    
    // Clean up
    sqlx::query("DELETE FROM agents WHERE agent_id = $1")
        .bind(agent_id)
        .execute(&state.pool)
        .await
        .expect("Failed to clean up test agent");
}

#[tokio::test]
async fn test_method_not_found() {
    let state = setup_test_state().await;
    
    // Test unknown method
    let request = JsonRpcRequest {
        jsonrpc: "2.0".to_string(),
        method: "unknown_method".to_string(),
        params: None,
        id: Some(json!(1)),
    };
    
    let request_body = serde_json::to_string(&request).unwrap();
    let response = handle_mcp(State(state), request_body).await;
    
    assert!(response.is_ok(), "Should return error response");
    let result = response.unwrap();
    assert!(result.error.is_some(), "Should return method not found error");
    assert_eq!(result.error.unwrap().code, -32602, "Should return method not found error code");
}

#[tokio::test]
async fn test_batch_requests_not_supported() {
    let state = setup_test_state().await;
    
    // Test batch request
    let batch_request = json!([
        {
            "jsonrpc": "2.0",
            "method": "search_atoms",
            "params": {"domain": "test"},
            "id": 1
        },
        {
            "jsonrpc": "2.0",
            "method": "search_atoms",
            "params": {"domain": "test2"},
            "id": 2
        }
    ]);
    
    let request_body = batch_request.to_string();
    let response = handle_mcp(State(state), request_body).await;
    
    assert!(response.is_ok(), "Should return error response");
    let result = response.unwrap();
    assert!(result.error.is_some(), "Should return batch not supported error");
    assert_eq!(result.error.unwrap().code, -32700, "Should return parse error for batch requests");
}
