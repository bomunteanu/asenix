// Integration tests for review queue functionality

use super::{setup_test_app, make_http_request};
use serial_test::serial;
use serde_json::json;

#[serial]
#[tokio::test]
async fn test_review_queue_end_to_end() {
    let app = setup_test_app().await;
    
    // Step 1: Register an agent
    let (status, body) = make_http_request(
        &app,
        axum::http::Method::POST,
        "/rpc",
        Some(r#"{
            "jsonrpc": "2.0",
            "method": "register_agent_simple",
            "params": {
                "agent_name": "review-test-agent"
            },
            "id": 1
        }"#)
    ).await
    .expect("Failed to register agent");
    
    assert_eq!(status, axum::http::StatusCode::OK);
    
    let agent_data: serde_json::Value = serde_json::from_str(&body)
        .expect("Failed to parse agent response");
    let agent_id = agent_data["result"]["agent_id"].as_str().unwrap();
    let api_token = agent_data["result"]["api_token"].as_str().unwrap();
    
    // Step 2: Publish a test atom (should be pending review)
    let (status, body) = make_http_request(
        &app,
        axum::http::Method::POST,
        "/rpc",
        Some(&json!({
            "jsonrpc": "2.0",
            "method": "publish_atoms",
            "params": {
                "agent_id": agent_id,
                "api_token": api_token,
                "atoms": [{
                    "atom_type": "finding",
                    "domain": "review_test_domain",
                    "statement": "Test finding for review queue integration test",
                    "conditions": {"test": true},
                    "metrics": {"accuracy": 0.95},
                    "provenance": {"test": "integration"},
                    "signature": "deadbeefdeadbeefdeadbeefdeadbeefdeadbeefdeadbeefdeadbeefdeadbeefdeadbeefdeadbeefdeadbeefdeadbeefdeadbeefdeadbeefdeadbeefdeadbeefdeadbeefdeadbeefdeadbeefdeadbeefdeadbeef"
                }]
            },
            "id": 2
        }).to_string())
    ).await
    .expect("Failed to publish atom");
    
    assert_eq!(status, axum::http::StatusCode::OK);
    
    let publish_data: serde_json::Value = serde_json::from_str(&body)
        .expect("Failed to parse publish response");
    
    // Check for error in response (error field being null means success)
    if publish_data.get("error").and_then(|e| e.as_str()).is_some() {
        panic!("Publish failed with error: {}", publish_data["error"]);
    }
    
    let atom_ids = publish_data["result"]["published_atoms"].as_array().unwrap();
    assert!(!atom_ids.is_empty());
    
    let atom_id = atom_ids[0].as_str().unwrap();
    
    // Step 3: Check the review queue (should contain our atom)
    let (status, body) = make_http_request(
        &app,
        axum::http::Method::GET,
        "/review?limit=10&offset=0",
        None
    ).await
    .expect("Failed to get review queue");
    
    assert_eq!(status, axum::http::StatusCode::OK);
    
    let queue_data: serde_json::Value = serde_json::from_str(&body)
        .expect("Failed to parse queue response");
    let items = queue_data["items"].as_array().unwrap();
    
    // Find our atom in the queue
    let our_atom = items.iter().find(|item| {
        item["atom_id"].as_str() == Some(atom_id)
    });
    
    assert!(our_atom.is_some(), "Atom should be in review queue");
    
    let atom_in_queue = our_atom.unwrap();
    assert_eq!(atom_in_queue["review_status"], "pending");
    assert_eq!(atom_in_queue["atom_type"], "finding");
    assert_eq!(atom_in_queue["domain"], "review_test_domain");
    
    // Step 4: Approve the atom
    let (status, body) = make_http_request(
        &app,
        axum::http::Method::POST,
        &format!("/review/{}", atom_id),
        Some(&json!({
            "action": "approve",
            "reason": "Integration test approval"
        }).to_string())
    ).await
    .expect("Failed to review atom");
    
    assert_eq!(status, axum::http::StatusCode::OK);
    
    let review_data: serde_json::Value = serde_json::from_str(&body)
        .expect("Failed to parse review response");
    assert_eq!(review_data["status"], "approve");
    assert_eq!(review_data["atom_id"], atom_id);
    assert!(review_data["review_id"].as_str().is_some());
    
    // Step 5: Check that atom is no longer in pending queue
    let (status, body) = make_http_request(
        &app,
        axum::http::Method::GET,
        "/review?limit=10&offset=0",
        None
    ).await
    .expect("Failed to get review queue after review");
    
    assert_eq!(status, axum::http::StatusCode::OK);
    
    let queue_after_data: serde_json::Value = serde_json::from_str(&body)
        .expect("Failed to parse queue after response");
    let items_after = queue_after_data["items"].as_array().unwrap();
    
    let our_atom_after = items_after.iter().find(|item| {
        item["atom_id"].as_str() == Some(atom_id)
    });
    
    assert!(our_atom_after.is_none(), "Atom should no longer be in pending queue after approval");
}

#[serial]
#[tokio::test]
async fn test_review_queue_rejection() {
    let app = setup_test_app().await;
    
    // Register agent
    let (status, body) = make_http_request(
        &app,
        axum::http::Method::POST,
        "/rpc",
        Some(r#"{
            "jsonrpc": "2.0",
            "method": "register_agent_simple",
            "params": {
                "agent_name": "review-reject-test-agent"
            },
            "id": 1
        }"#)
    ).await
    .expect("Failed to register agent");
    
    let agent_data: serde_json::Value = serde_json::from_str(&body)
        .expect("Failed to parse agent response");
    let agent_id = agent_data["result"]["agent_id"].as_str().unwrap();
    let api_token = agent_data["result"]["api_token"].as_str().unwrap();
    
    // Publish atom
    let (status, body) = make_http_request(
        &app,
        axum::http::Method::POST,
        "/rpc",
        Some(&json!({
            "jsonrpc": "2.0",
            "method": "publish_atoms",
            "params": {
                "agent_id": agent_id,
                "api_token": api_token,
                "atoms": [{
                    "atom_type": "hypothesis",
                    "domain": "review_reject_test",
                    "statement": "Test hypothesis for rejection workflow",
                    "conditions": {"test": true},
                    "provenance": {"test": "integration"},
                    "signature": "deadbeefdeadbeefdeadbeefdeadbeefdeadbeefdeadbeefdeadbeefdeadbeefdeadbeefdeadbeefdeadbeefdeadbeefdeadbeefdeadbeefdeadbeefdeadbeefdeadbeefdeadbeefdeadbeefdeadbeefdeadbeef"
                }]
            },
            "id": 2
        }).to_string())
    ).await
    .expect("Failed to publish atom");
    
    let publish_data: serde_json::Value = serde_json::from_str(&body)
        .expect("Failed to parse publish response");
    let atom_id = publish_data["result"]["published_atoms"][0].as_str().unwrap();
    
    // Reject the atom
    let (status, body) = make_http_request(
        &app,
        axum::http::Method::POST,
        &format!("/review/{}", atom_id),
        Some(&json!({
            "action": "reject",
            "reason": "Test rejection for integration test"
        }).to_string())
    ).await
    .expect("Failed to review atom");
    
    assert_eq!(status, axum::http::StatusCode::OK);
    
    let review_data: serde_json::Value = serde_json::from_str(&body)
        .expect("Failed to parse review response");
    assert_eq!(review_data["status"], "reject");
    assert_eq!(review_data["reason"], "Test rejection for integration test");
}

#[serial]
#[tokio::test]
async fn test_review_queue_empty() {
    let app = setup_test_app().await;
    
    // Check empty review queue
    let (status, body) = make_http_request(
        &app,
        axum::http::Method::GET,
        "/review?limit=10&offset=0",
        None
    ).await
    .expect("Failed to get review queue");
    
    assert_eq!(status, axum::http::StatusCode::OK);
    
    let queue_data: serde_json::Value = serde_json::from_str(&body)
        .expect("Failed to parse queue response");
    let items = queue_data["items"].as_array().unwrap();
    assert_eq!(items.len(), 0);
    assert_eq!(queue_data["total"], 0);
}

#[serial]
#[tokio::test]
async fn test_review_queue_invalid_action() {
    let app = setup_test_app().await;
    
    // Try to review with invalid action
    let (status, body) = make_http_request(
        &app,
        axum::http::Method::POST,
        "/review/nonexistent-atom",
        Some(&json!({
            "action": "invalid_action",
            "reason": "This should fail"
        }).to_string())
    ).await
    .expect("Failed to make review request");
    
    assert_eq!(status, axum::http::StatusCode::BAD_REQUEST);
}

#[serial]
#[tokio::test]
async fn test_review_queue_nonexistent_atom() {
    let app = setup_test_app().await;
    
    // Try to review nonexistent atom
    let (status, body) = make_http_request(
        &app,
        axum::http::Method::POST,
        "/review/nonexistent-atom-id",
        Some(&json!({
            "action": "approve",
            "reason": "This should fail"
        }).to_string())
    ).await
    .expect("Failed to make review request");
    
    assert_eq!(status, axum::http::StatusCode::NOT_FOUND);
}
