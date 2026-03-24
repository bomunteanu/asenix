// Integration tests module

mod publish_atoms_tests;
mod query_cluster_tests;
mod health_tests;
mod schema_tests;
mod mcp_tools_tests;
mod sse_tests;
mod review_queue_tests;
mod full_workflow_tests;
mod artifact_unification_tests;
mod artifact_processing_tests;
mod project_tests;

use axum::Router;
use axum::http::{Request, Method};
use axum::body::Body;
use tower::ServiceExt;
use std::env;
use std::sync::Arc;
use tokio::sync::mpsc;
use tokio::sync::broadcast;
use serde_json::json;
use asenix::config::Config;
use asenix::state::AppState;
use asenix::api;
use asenix::db::pool::create_pool;
use asenix::storage::LocalStorage;

/// Default test database URL, overridable via DATABASE_URL env var.
pub fn test_database_url() -> String {
    env::var("DATABASE_URL")
        .unwrap_or_else(|_| "postgres://asenix:asenix_password@localhost:5432/asenix_test".to_string())
}

/// Test helper that sets up a clean database and returns a router ready for testing
pub async fn setup_test_app() -> Router {
    let database_url = test_database_url();
    
    let pool = sqlx::PgPool::connect(&database_url)
        .await
        .expect("Failed to connect to database");

    // Clean up database before each test
    if let Err(e) = truncate_all_tables(&pool).await {
        eprintln!("Warning: Failed to cleanup database: {}", e);
    }
    
    let config = Config {
        hub: asenix::config::HubConfig {
            name: "test-hub".to_string(),
            domain: "test.asenix".to_string(),
            listen_address: "127.0.0.1:8080".to_string(),
            embedding_endpoint: "http://localhost:11434".to_string(),
            embedding_model: "nomic-embed-text".to_string(),
            embedding_dimension: 768,
            structured_vector_reserved_dims: 10,
            dims_per_numeric_key: 2,
            dims_per_categorical_key: 1,
            neighbourhood_radius: 0.1,
            summary_llm_endpoint: Some("http://localhost:11434".to_string()),
            summary_llm_model: Some("llama2".to_string()),
            artifact_storage_path: "./test_artifacts".to_string(),
            max_artifact_blob_bytes: 1048576,  // 1MB for tests
            max_artifact_storage_per_agent_bytes: 10485760,  // 10MB for tests
        },
        pheromone: asenix::config::PheromoneConfig {
            decay_half_life_hours: 24,
            attraction_cap: 10.0,
            novelty_radius: 0.5,
            disagreement_threshold: 0.8,
            exploration_samples: 10,
            exploration_density_radius: 0.5,
        },
        trust: asenix::config::TrustConfig {
            reliability_threshold: 0.7,
            independence_ancestry_depth: 5,
            probation_atom_count: 10,
            max_atoms_per_hour: 100,
        },
        workers: asenix::config::WorkersConfig {
            embedding_pool_size: 4,
            decay_interval_minutes: 60,
            claim_ttl_hours: 24,
            staleness_check_interval_minutes: 30,
            bounty_needed_novelty_threshold: 0.7,
            bounty_sparse_region_max_atoms: 3,
            lifecycle_check_interval_minutes: 60,
            metrics_collection_interval_seconds: 30,
        },
        acceptance: asenix::config::AcceptanceConfig {
            required_provenance_fields: vec!["agent_id".to_string(), "timestamp".to_string()],
        },
        mcp: asenix::config::McpConfig {
            allowed_origins: vec!["http://localhost:3000".to_string(), "https://localhost:3000".to_string()],
        },
    };

    // Create database pool
    let pool = create_pool(&config, &database_url).await.expect("Failed to create test database pool");

    // Create reviews table if it doesn't exist (for review queue tests)
    create_reviews_table(&pool).await.expect("Failed to create reviews table");

    // Truncate all tables for clean state
    truncate_all_tables(&pool).await.expect("Failed to truncate tables");

    // Create application state
    let (sse_broadcast_tx, _sse_broadcast_rx) = broadcast::channel(1000);

    let storage = Arc::new(LocalStorage::new(
        std::path::PathBuf::from("./test_artifacts")
    ));

    let (embedding_tx, _embedding_rx) = tokio::sync::mpsc::channel::<String>(100);
    let state = AppState::new(pool, Arc::new(config), sse_broadcast_tx, storage, embedding_tx).await
        .expect("Failed to create app state");

    // Build router
    Router::new()
        .route("/health", axum::routing::get(api::handlers::health_check))
        .route("/metrics", axum::routing::get(api::handlers::metrics))
        .route("/review", axum::routing::get(api::handlers::get_review_queue))
        .route("/review/:id", axum::routing::post(api::handlers::review_atom))
        .route("/events", axum::routing::get(api::sse::sse_events))
        .route("/rpc", axum::routing::post(api::rpc::handle_mcp))
        .route("/mcp", axum::routing::post(api::mcp_server::handle_mcp_request)
            .delete(api::mcp_server::handle_mcp_delete))
        .layer(axum::extract::DefaultBodyLimit::max(10 * 1024 * 1024)) // 10MB limit
        .with_state(Arc::new(state))
}

async fn truncate_all_tables(pool: &sqlx::PgPool) -> Result<(), sqlx::Error> {
    // Truncate tables in correct order to respect foreign key constraints
    let tables = vec![
        "edges",
        "synthesis",
        "bounties", 
        "reviews",
        "claims",
        "atoms", 
        "agents",
        "condition_registry",
    ];

    for table in tables {
        sqlx::query(&format!("TRUNCATE TABLE {} CASCADE", table))
            .execute(pool)
            .await?;
    }

    Ok(())
}

async fn create_reviews_table(pool: &sqlx::PgPool) -> Result<(), sqlx::Error> {
    // Create reviews table if it doesn't exist
    sqlx::query(
        "CREATE TABLE IF NOT EXISTS reviews (
            review_id TEXT PRIMARY KEY,
            atom_id TEXT NOT NULL REFERENCES atoms(atom_id),
            reviewer_agent_id TEXT NOT NULL REFERENCES agents(agent_id),
            decision TEXT NOT NULL CHECK (decision IN ('approve', 'reject', 'auto_approve')),
            reason TEXT NULL,
            created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
            UNIQUE(atom_id, reviewer_agent_id)
        )"
    )
    .execute(pool)
    .await?;
    
    // Add review_status column to atoms table if it doesn't exist
    sqlx::query(
        "ALTER TABLE atoms ADD COLUMN IF NOT EXISTS review_status TEXT NOT NULL DEFAULT 'pending' CHECK (review_status IN ('pending', 'approved', 'rejected', 'auto_approved'))"
    )
    .execute(pool)
    .await?;
    
    Ok(())
}

/// Helper to make JSON-RPC requests to the test router (with MCP-required headers)
pub async fn make_mcp_request(
    router: &Router,
    method: &str,
    params: Option<serde_json::Value>,
    id: Option<serde_json::Value>,
) -> Result<serde_json::Value, Box<dyn std::error::Error>> {
    let request = json!({
        "jsonrpc": "2.0",
        "method": method,
        "params": params,
        "id": id
    });

    let response = router
        .clone()
        .oneshot(
            Request::builder()
                .method(Method::POST)
                .uri("/mcp")
                .header("origin", "http://localhost:3000")
                .header("accept", "application/json, text/event-stream")
                .header("content-type", "application/json")
                .body(Body::from(request.to_string()))
                .unwrap()
        )
        .await
        .unwrap();

    let (_parts, body) = response.into_parts();
    let body_bytes = axum::body::to_bytes(body, usize::MAX).await.unwrap();
    let response_text = String::from_utf8(body_bytes.to_vec()).unwrap();

    let response_json: serde_json::Value = serde_json::from_str(&response_text)?;
    Ok(response_json)
}

/// Initialize an MCP session and return the session ID
pub async fn initialize_session(router: &Router) -> String {
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

    let response = router
        .clone()
        .oneshot(
            Request::builder()
                .method(Method::POST)
                .uri("/mcp")
                .header("origin", "http://localhost:3000")
                .header("accept", "application/json, text/event-stream")
                .header("content-type", "application/json")
                .body(Body::from(init_request.to_string()))
                .unwrap()
        )
        .await
        .unwrap();

    let session_id = response.headers().get("mcp-session-id")
        .expect("Session ID should be returned")
        .to_str()
        .unwrap()
        .to_string();

    // Send initialized notification
    let init_notify = json!({
        "jsonrpc": "2.0",
        "method": "notifications/initialized",
        "params": {}
    });

    router
        .clone()
        .oneshot(
            Request::builder()
                .method(Method::POST)
                .uri("/mcp")
                .header("origin", "http://localhost:3000")
                .header("accept", "application/json, text/event-stream")
                .header("mcp-session-id", &session_id)
                .header("content-type", "application/json")
                .body(Body::from(init_notify.to_string()))
                .unwrap()
        )
        .await
        .unwrap();

    session_id
}

/// Call an MCP tool via tools/call, returning a response shaped like the old
/// direct-dispatch format: { "jsonrpc": "2.0", "result": <inner>, "error": null, "id": <id> }
pub async fn make_tool_call(
    router: &Router,
    session_id: &str,
    tool_name: &str,
    arguments: serde_json::Value,
    id: serde_json::Value,
) -> Result<serde_json::Value, Box<dyn std::error::Error>> {
    let request_body = json!({
        "jsonrpc": "2.0",
        "method": "tools/call",
        "params": {
            "name": tool_name,
            "arguments": arguments
        },
        "id": id
    });

    let response = router
        .clone()
        .oneshot(
            Request::builder()
                .method(Method::POST)
                .uri("/mcp")
                .header("origin", "http://localhost:3000")
                .header("accept", "application/json, text/event-stream")
                .header("mcp-session-id", session_id)
                .header("content-type", "application/json")
                .body(Body::from(request_body.to_string()))
                .unwrap()
        )
        .await
        .unwrap();

    let body = axum::body::to_bytes(response.into_body(), usize::MAX).await.unwrap();
    let response_json: serde_json::Value = serde_json::from_slice(&body)?;

    // Check for JSON-RPC level error
    if let Some(error) = response_json.get("error") {
        if !error.is_null() {
            return Ok(json!({
                "jsonrpc": "2.0",
                "error": error,
                "id": response_json.get("id")
            }));
        }
    }

    // Parse ToolCallResult from result field
    let result = response_json.get("result")
        .ok_or("Missing result field")?;
    let is_error = result.get("isError")
        .and_then(|v| v.as_bool())
        .unwrap_or(false);
    let content = result.get("content")
        .and_then(|v| v.as_array())
        .ok_or("Missing content array")?;
    let text = content.first()
        .and_then(|c| c.get("text"))
        .and_then(|v| v.as_str())
        .ok_or("Missing text in content")?;

    if is_error {
        Ok(json!({
            "jsonrpc": "2.0",
            "error": {
                "code": -32000,
                "message": text
            },
            "id": id
        }))
    } else {
        let inner_result: serde_json::Value = serde_json::from_str(text)
            .unwrap_or_else(|_| json!(text));
        Ok(json!({
            "jsonrpc": "2.0",
            "result": inner_result,
            "error": null,
            "id": id
        }))
    }
}

/// Helper to make HTTP requests to the test router
pub async fn make_http_request(
    router: &Router,
    method: axum::http::Method,
    path: &str,
    body: Option<&str>,
) -> Result<(axum::http::StatusCode, String), Box<dyn std::error::Error>> {
    let mut request_builder = axum::http::Request::builder()
        .method(method)
        .uri(path);

    if let Some(body_content) = body {
        request_builder = request_builder.header(axum::http::header::CONTENT_TYPE, "application/json");
        let body_string = body_content.to_string();
        let request = request_builder.body(axum::body::Body::from(body_string)).unwrap();
        
        let response = router.clone().oneshot(request).await.unwrap();
        let (parts, body) = response.into_parts();
        let body_bytes = axum::body::to_bytes(body, usize::MAX).await.unwrap();
        let body_text = String::from_utf8(body_bytes.to_vec()).unwrap();
        
        Ok((parts.status, body_text))
    } else {
        let request = request_builder.body(axum::body::Body::empty()).unwrap();
        
        let response = router.clone().oneshot(request).await.unwrap();
        let (parts, body) = response.into_parts();
        let body_bytes = axum::body::to_bytes(body, usize::MAX).await.unwrap();
        let body_text = String::from_utf8(body_bytes.to_vec()).unwrap();
        
        Ok((parts.status, body_text))
    }
}

