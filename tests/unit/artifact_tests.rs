use axum::{
    body::Body,
    http::{Request, StatusCode},
};
use serde_json::json;
use tower::ServiceExt;

use asenix::api::artifacts::*;
use asenix::config::{Config, HubConfig};
use asenix::state::AppState;
use asenix::storage::LocalStorage;
use sqlx::PgPool;

async fn setup_test_app() -> axum::Router {
    // Use a temporary directory for test artifacts
    let temp_dir = std::env::temp_dir().join("asenix_artifact_tests");
    tokio::fs::create_dir_all(&temp_dir).await.unwrap();

    // Create test storage
    let storage = std::sync::Arc::new(LocalStorage::new(temp_dir));

    // Create test config
    let config = Config {
        hub: HubConfig {
            listen_address: "127.0.0.1:0".to_string(),
            database_url: "postgresql://test".to_string(),
            artifact_storage_path: temp_dir.to_string_lossy().to_string(),
            max_blob_size: 1024 * 1024, // 1MB
            max_storage_per_agent: 10 * 1024 * 1024, // 10MB
        },
        pheromone: Default::default(),
        trust: Default::default(),
        workers: Default::default(),
        acceptance: Default::default(),
    };

    // Create a mock pool (we'll use a simple in-memory setup for unit tests)
    // For actual database operations, we'd need a test database
    let pool = PgPool::connect("postgresql://test")
        .await
        .expect("Failed to create test database pool");

    let state = AppState::new(
        pool,
        std::sync::Arc::new(config),
        tokio::sync::mpsc::channel(100).0,
        tokio::sync::broadcast::channel(100).0,
        storage,
    )
    .await
    .unwrap();

    axum::Router::new()
        .route("/artifacts/:hash", axum::routing::put(put_artifact))
        .route("/artifacts/:hash", axum::routing::get(get_artifact))
        .route("/artifacts/:hash", axum::routing::head(head_artifact))
        .route("/artifacts/:hash/meta", axum::routing::get(get_artifact_metadata))
        .route("/artifacts/:hash/ls", axum::routing::get(list_artifact_tree))
        .route("/artifacts/:hash/resolve/*path", axum::routing::get(resolve_artifact_path))
        .with_state(std::sync::Arc::new(state))
}

#[tokio::test]
async fn test_put_artifact_blob() {
    let app = setup_test_app().await;
    
    let test_data = b"Hello, World!";
    let hash = blake3::hash(test_data).to_hex().to_string();
    
    let request = Request::builder()
        .method("PUT")
        .uri(format!("/artifacts/{}", hash))
        .header("content-type", "application/octet-stream")
        .header("x-agent-id", "test_agent")
        .header("x-signature", "deadbeef") // Mock signature
        .body(Body::from(test_data.to_vec()))
        .unwrap();

    let response = app.oneshot(request).await.unwrap();
    
    assert_eq!(response.status(), StatusCode::OK);
}

#[tokio::test]
async fn test_put_artifact_tree() {
    let app = setup_test_app().await;
    
    let tree_manifest = json!({
        "type": "tree",
        "entries": [
            {
                "path": "data.txt",
                "hash": "deadbeefdeadbeefdeadbeefdeadbeefdeadbeefdeadbeefdeadbeefdeadbeef",
                "size": 13,
                "type": "blob"
            }
        ]
    });
    
    let tree_data = serde_json::to_vec(&tree_manifest).unwrap();
    let hash = blake3::hash(&tree_data).to_hex().to_string();
    
    let request = Request::builder()
        .method("PUT")
        .uri(format!("/artifacts/{}", hash))
        .header("content-type", "application/json")
        .header("x-agent-id", "test_agent")
        .header("x-signature", "deadbeef") // Mock signature
        .body(Body::from(tree_data))
        .unwrap();

    let response = app.oneshot(request).await.unwrap();
    
    assert_eq!(response.status(), StatusCode::OK);
}

#[tokio::test]
async fn test_get_artifact_blob() {
    let app = setup_test_app().await;
    
    // First put an artifact
    let test_data = b"Hello, World!";
    let hash = blake3::hash(test_data).to_hex().to_string();
    
    let put_request = Request::builder()
        .method("PUT")
        .uri(format!("/artifacts/{}", hash))
        .header("content-type", "application/octet-stream")
        .header("x-agent-id", "test_agent")
        .header("x-signature", "deadbeef")
        .body(Body::from(test_data.to_vec()))
        .unwrap();

    let put_response = app.oneshot(put_request).await.unwrap();
    assert_eq!(put_response.status(), StatusCode::OK);
    
    // Then get it
    let get_request = Request::builder()
        .method("GET")
        .uri(format!("/artifacts/{}", hash))
        .body(Body::empty())
        .unwrap();

    let get_response = app.oneshot(get_request).await.unwrap();
    assert_eq!(get_response.status(), StatusCode::OK);
    
    let body = axum::body::to_bytes(get_response.into_body(), usize::MAX).await.unwrap();
    assert_eq!(body, test_data);
}

#[tokio::test]
async fn test_head_artifact_exists() {
    let app = setup_test_app().await;
    
    // First put an artifact
    let test_data = b"Hello, World!";
    let hash = blake3::hash(test_data).to_hex().to_string();
    
    let put_request = Request::builder()
        .method("PUT")
        .uri(format!("/artifacts/{}", hash))
        .header("content-type", "application/octet-stream")
        .header("x-agent-id", "test_agent")
        .header("x-signature", "deadbeef")
        .body(Body::from(test_data.to_vec()))
        .unwrap();

    let put_response = app.oneshot(put_request).await.unwrap();
    assert_eq!(put_response.status(), StatusCode::OK);
    
    // Then check if it exists with HEAD
    let head_request = Request::builder()
        .method("HEAD")
        .uri(format!("/artifacts/{}", hash))
        .body(Body::empty())
        .unwrap();

    let head_response = app.oneshot(head_request).await.unwrap();
    assert_eq!(head_response.status(), StatusCode::OK);
    
    // Should have content-length header
    assert!(head_response.headers().contains_key("content-length"));
    assert_eq!(
        head_response.headers().get("content-length").unwrap(),
        "13"
    );
}

#[tokio::test]
async fn test_get_artifact_metadata() {
    let app = setup_test_app().await;
    
    // First put an artifact
    let test_data = b"Hello, World!";
    let hash = blake3::hash(test_data).to_hex().to_string();
    
    let put_request = Request::builder()
        .method("PUT")
        .uri(format!("/artifacts/{}", hash))
        .header("content-type", "application/octet-stream")
        .header("x-agent-id", "test_agent")
        .header("x-signature", "deadbeef")
        .body(Body::from(test_data.to_vec()))
        .unwrap();

    let put_response = app.oneshot(put_request).await.unwrap();
    assert_eq!(put_response.status(), StatusCode::OK);
    
    // Then get metadata
    let meta_request = Request::builder()
        .method("GET")
        .uri(format!("/artifacts/{}/meta", hash))
        .body(Body::empty())
        .unwrap();

    let meta_response = app.oneshot(meta_request).await.unwrap();
    assert_eq!(meta_response.status(), StatusCode::OK);
    
    let body = axum::body::to_bytes(meta_response.into_body(), usize::MAX).await.unwrap();
    let metadata: serde_json::Value = serde_json::from_slice(&body).unwrap();
    
    assert_eq!(metadata["hash"], hash);
    assert_eq!(metadata["type"], "blob");
    assert_eq!(metadata["size"], 13);
}

#[tokio::test]
async fn test_list_artifact_tree() {
    let app = setup_test_app().await;
    
    // Create a tree manifest
    let tree_manifest = json!({
        "type": "tree",
        "entries": [
            {
                "path": "data.txt",
                "hash": "deadbeefdeadbeefdeadbeefdeadbeefdeadbeefdeadbeefdeadbeefdeadbeef",
                "size": 13,
                "type": "blob"
            },
            {
                "path": "subdir/config.json",
                "hash": "cafebabecafebabecafebabecafebabecafebabecafebabecafebabecafebabe",
                "size": 42,
                "type": "blob"
            }
        ]
    });
    
    let tree_data = serde_json::to_vec(&tree_manifest).unwrap();
    let hash = blake3::hash(&tree_data).to_hex().to_string();
    
    // Put the tree
    let put_request = Request::builder()
        .method("PUT")
        .uri(format!("/artifacts/{}", hash))
        .header("content-type", "application/json")
        .header("x-agent-id", "test_agent")
        .header("x-signature", "deadbeef")
        .body(Body::from(tree_data))
        .unwrap();

    let put_response = app.oneshot(put_request).await.unwrap();
    assert_eq!(put_response.status(), StatusCode::OK);
    
    // List the tree contents
    let ls_request = Request::builder()
        .method("GET")
        .uri(format!("/artifacts/{}/ls", hash))
        .body(Body::empty())
        .unwrap();

    let ls_response = app.oneshot(ls_request).await.unwrap();
    assert_eq!(ls_response.status(), StatusCode::OK);
    
    let body = axum::body::to_bytes(ls_response.into_body(), usize::MAX).await.unwrap();
    let listing: serde_json::Value = serde_json::from_slice(&body).unwrap();
    
    assert_eq!(listing["entries"].as_array().unwrap().len(), 2);
    assert_eq!(listing["entries"][0]["path"], "data.txt");
    assert_eq!(listing["entries"][1]["path"], "subdir/config.json");
}

#[tokio::test]
async fn test_resolve_artifact_path() {
    let app = setup_test_app().await;
    
    // Create a tree manifest
    let tree_manifest = json!({
        "type": "tree",
        "entries": [
            {
                "path": "data.txt",
                "hash": "deadbeefdeadbeefdeadbeefdeadbeefdeadbeefdeadbeefdeadbeefdeadbeef",
                "size": 13,
                "type": "blob"
            }
        ]
    });
    
    let tree_data = serde_json::to_vec(&tree_manifest).unwrap();
    let hash = blake3::hash(&tree_data).to_hex().to_string();
    
    // Put the tree
    let put_request = Request::builder()
        .method("PUT")
        .uri(format!("/artifacts/{}", hash))
        .header("content-type", "application/json")
        .header("x-agent-id", "test_agent")
        .header("x-signature", "deadbeef")
        .body(Body::from(tree_data))
        .unwrap();

    let put_response = app.oneshot(put_request).await.unwrap();
    assert_eq!(put_response.status(), StatusCode::OK);
    
    // Resolve a path in the tree
    let resolve_request = Request::builder()
        .method("GET")
        .uri(format!("/artifacts/{}/resolve/data.txt", hash))
        .body(Body::empty())
        .unwrap();

    let resolve_response = app.oneshot(resolve_request).await.unwrap();
    assert_eq!(resolve_response.status(), StatusCode::OK);
    
    let body = axum::body::to_bytes(resolve_response.into_body(), usize::MAX).await.unwrap();
    let resolved: serde_json::Value = serde_json::from_slice(&body).unwrap();
    
    assert_eq!(resolved["path"], "data.txt");
    assert_eq!(resolved["hash"], "deadbeefdeadbeefdeadbeefdeadbeefdeadbeefdeadbeefdeadbeefdeadbeef");
    assert_eq!(resolved["size"], 13);
    assert_eq!(resolved["type"], "blob");
}

#[tokio::test]
async fn test_get_nonexistent_artifact() {
    let app = setup_test_app().await;
    
    let request = Request::builder()
        .method("GET")
        .uri("/artifacts/nonexistenthash")
        .body(Body::empty())
        .unwrap();

    let response = app.oneshot(request).await.unwrap();
    assert_eq!(response.status(), StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn test_put_invalid_hash() {
    let app = setup_test_app().await;
    
    let test_data = b"Hello, World!";
    let wrong_hash = "wronghash123456789012345678901234567890123456789012345678901234567890";
    
    let request = Request::builder()
        .method("PUT")
        .uri(format!("/artifacts/{}", wrong_hash))
        .header("content-type", "application/octet-stream")
        .header("x-agent-id", "test_agent")
        .header("x-signature", "deadbeef")
        .body(Body::from(test_data.to_vec()))
        .unwrap();

    let response = app.oneshot(request).await.unwrap();
    assert_eq!(response.status(), StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn test_put_missing_headers() {
    let app = setup_test_app().await;
    
    let test_data = b"Hello, World!";
    let hash = blake3::hash(test_data).to_hex().to_string();
    
    // Missing agent_id header
    let request = Request::builder()
        .method("PUT")
        .uri(format!("/artifacts/{}", hash))
        .header("content-type", "application/octet-stream")
        .header("x-signature", "deadbeef")
        .body(Body::from(test_data.to_vec()))
        .unwrap();

    let response = app.oneshot(request).await.unwrap();
    assert_eq!(response.status(), StatusCode::BAD_REQUEST);
}
