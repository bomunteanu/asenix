//! Integration tests for artifact unification — inline artifact uploads via publish_atoms.

use super::setup_test_app;
use axum::http::{Request, Method};
use axum::body::Body;
use base64::{Engine as _, engine::general_purpose::STANDARD as BASE64};
use serde_json::json;
use serial_test::serial;
use tower::ServiceExt;

/// POST to /rpc with a JSON-RPC body and return the parsed response.
async fn rpc_call(
    app: &axum::Router,
    method: &str,
    params: serde_json::Value,
    id: i64,
) -> serde_json::Value {
    let body = json!({
        "jsonrpc": "2.0",
        "method": method,
        "params": params,
        "id": id
    });
    let response = app
        .clone()
        .oneshot(
            Request::builder()
                .method(Method::POST)
                .uri("/rpc")
                .header("Content-Type", "application/json")
                .body(Body::from(body.to_string()))
                .unwrap(),
        )
        .await
        .unwrap();
    let bytes = axum::body::to_bytes(response.into_body(), usize::MAX).await.unwrap();
    serde_json::from_slice(&bytes).unwrap()
}

#[serial]
#[tokio::test]
async fn test_inline_artifact_upload_via_publish_atoms() {
    let app = setup_test_app().await;

    // Register agent
    let reg = rpc_call(&app, "register_agent_simple", json!({"agent_name": "test-artifact-agent"}), 1).await;
    assert!(reg["error"].is_null(), "register failed: {}", reg["error"]);
    let agent_id = reg["result"]["agent_id"].as_str().expect("agent_id missing").to_string();
    let api_token = reg["result"]["api_token"].as_str().expect("api_token missing").to_string();

    // Publish atom with inline blob artifact
    let file_content = b"Test file content for artifact upload.";
    let encoded = BASE64.encode(file_content);

    let pub_resp = rpc_call(&app, "publish_atoms", json!({
        "agent_id": agent_id,
        "api_token": api_token,
        "atoms": [{
            "atom_type": "finding",
            "domain": "test-domain",
            "statement": "Test finding with inline artifact",
            "conditions": { "experiment": "test" },
            "metrics": { "accuracy": 0.95 },
            "provenance": { "agent_id": agent_id, "timestamp": "2026-01-01T00:00:00Z" },
            "artifact_inline": {
                "artifact_type": "blob",
                "content": { "data": encoded },
                "media_type": "text/plain"
            }
        }]
    }), 2).await;

    assert!(pub_resp["error"].is_null(), "publish failed: {}", pub_resp["error"]);
    let atoms = pub_resp["result"]["published_atoms"].as_array().expect("published_atoms should be array");
    assert_eq!(atoms.len(), 1, "expected 1 published atom, got {}", atoms.len());
    // published_atoms contains atom ID strings
    assert!(atoms[0].is_string(), "expected atom ID string, got: {}", atoms[0]);
}

#[serial]
#[tokio::test]
async fn test_inline_tree_artifact_upload() {
    let app = setup_test_app().await;

    // Register agent
    let reg = rpc_call(&app, "register_agent_simple", json!({"agent_name": "test-tree-agent"}), 1).await;
    assert!(reg["error"].is_null(), "register failed: {}", reg["error"]);
    let agent_id = reg["result"]["agent_id"].as_str().expect("agent_id missing").to_string();
    let api_token = reg["result"]["api_token"].as_str().expect("api_token missing").to_string();

    let tree_entries = json!([
        { "name": "paper.pdf",    "hash": "a".repeat(64), "type_": "blob" },
        { "name": "data",         "hash": "b".repeat(64), "type_": "tree" },
        { "name": "code/main.py", "hash": "c".repeat(64), "type_": "blob" }
    ]);

    let pub_resp = rpc_call(&app, "publish_atoms", json!({
        "agent_id": agent_id,
        "api_token": api_token,
        "atoms": [{
            "atom_type": "hypothesis",
            "domain": "research",
            "statement": "Research hypothesis with supporting data package",
            "conditions": { "experiment": "multi-modal" },
            "metrics": { "confidence": 0.85 },
            "provenance": { "agent_id": agent_id, "timestamp": "2026-01-01T00:00:00Z" },
            "artifact_inline": {
                "artifact_type": "tree",
                "content": { "entries": tree_entries }
            }
        }]
    }), 2).await;

    assert!(pub_resp["error"].is_null(), "publish failed: {}", pub_resp["error"]);
    let atoms = pub_resp["result"]["published_atoms"].as_array().expect("published_atoms should be array");
    assert_eq!(atoms.len(), 1, "expected 1 published atom, got {}", atoms.len());
    assert!(atoms[0].is_string(), "expected atom ID string, got: {}", atoms[0]);
}
