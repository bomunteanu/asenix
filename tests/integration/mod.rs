// Integration tests module

mod health_tests;
mod schema_tests;
mod agent_registration_tests;
mod coordination_test_fixed;
mod mcp_lifecycle_tests;
mod mcp_tools_tests;

use axum::Router;
use tower::ServiceExt;
use std::env;
use std::sync::Arc;
use tokio::sync::mpsc;
use tokio::sync::broadcast;
use mote::config::Config;
use mote::state::AppState;
use mote::api;
use mote::db::pool::create_pool;
use mote::storage::LocalStorage;

/// Test helper that sets up a clean database and returns a router ready for testing
pub async fn setup_test_app() -> Router {
    // Load test configuration with flexible database URL
    let database_url = env::var("DATABASE_URL")
        .unwrap_or_else(|_| {
            // Try Docker Compose database first
            if env::var("DOCKER_ENV").unwrap_or_default() == "true" {
                "postgres://mote:mote_password@localhost:5432/mote".to_string()
            } else {
                // Fallback to local test database
                "postgresql://postgres:password@localhost:5432/mote_test".to_string()
            }
        });
    
    let pool = sqlx::PgPool::connect(&database_url)
        .await
        .expect("Failed to connect to database");

    // Clean up database before each test
    if let Err(e) = truncate_all_tables(&pool).await {
        eprintln!("Warning: Failed to cleanup database: {}", e);
    }
    
    let config = Config {
        hub: mote::config::HubConfig {
            name: "test-hub".to_string(),
            domain: "test.mote".to_string(),
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
        pheromone: mote::config::PheromoneConfig {
            decay_half_life_hours: 24,
            attraction_cap: 10.0,
            novelty_radius: 0.5,
            disagreement_threshold: 0.8,
        },
        trust: mote::config::TrustConfig {
            reliability_threshold: 0.7,
            independence_ancestry_depth: 5,
            probation_atom_count: 10,
            max_atoms_per_hour: 100,
        },
        workers: mote::config::WorkersConfig {
            embedding_pool_size: 4,
            decay_interval_minutes: 60,
            claim_ttl_hours: 168,
            staleness_check_interval_minutes: 30,
        },
        acceptance: mote::config::AcceptanceConfig {
            required_provenance_fields: vec!["agent_id".to_string(), "timestamp".to_string()],
        },
        mcp: mote::config::McpConfig {
            allowed_origins: vec!["http://localhost:3000".to_string(), "https://localhost:3000".to_string()],
        },
    };

    // Create database pool
    let pool = create_pool(&config, &database_url).await.expect("Failed to create test database pool");

    // Truncate all tables for clean state
    truncate_all_tables(&pool).await.expect("Failed to truncate tables");

    // Create application state
    let (embedding_queue_tx, _embedding_queue_rx) = mpsc::channel(1000);
    let (sse_broadcast_tx, _sse_broadcast_rx) = broadcast::channel(1000);
    
    let storage = Arc::new(LocalStorage::new(
        std::path::PathBuf::from("./test_artifacts")
    ));
    
    let state = AppState::new(pool, Arc::new(config), embedding_queue_tx, sse_broadcast_tx, storage).await
        .expect("Failed to create app state");

    // Build router
    Router::new()
        .route("/health", axum::routing::get(api::handlers::health_check))
        .route("/metrics", axum::routing::get(api::handlers::metrics))
        .route("/mcp", axum::routing::post(api::mcp_server::handle_mcp_request))
        .layer(axum::extract::DefaultBodyLimit::max(10 * 1024 * 1024)) // 10MB limit
        .with_state(Arc::new(state))
}

async fn truncate_all_tables(pool: &sqlx::PgPool) -> Result<(), sqlx::Error> {
    // Truncate tables in correct order to respect foreign key constraints
    let tables = vec![
        "edges",
        "synthesis",
        "bounties", 
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

/// Helper to make JSON-RPC requests to the test router
pub async fn make_mcp_request(
    router: &Router,
    method: &str,
    params: Option<serde_json::Value>,
    id: Option<serde_json::Value>,
) -> Result<serde_json::Value, Box<dyn std::error::Error>> {
    let request = serde_json::json!({
        "jsonrpc": "2.0",
        "method": method,
        "params": params,
        "id": id
    });

    let response = router
        .clone()
        .oneshot(
            axum::http::Request::builder()
                .method(axum::http::Method::POST)
                .uri("/mcp")
                .header(axum::http::header::CONTENT_TYPE, "application/json")
                .body(axum::body::Body::from(request.to_string()))
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
