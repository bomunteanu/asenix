//! Testing infrastructure for Phase 3 Mote tests
//! 
//! This module provides shared utilities for setting up test databases,
//! creating test agents, and building test applications with deterministic
//! configurations.

use ed25519_dalek::{Keypair, Signer};
use serde_json::{json, Value};
use sqlx::{PgPool, Row};
use std::collections::HashMap;
use std::sync::Arc;
use std::sync::atomic::{AtomicU32, Ordering};
use tokio::sync::{mpsc, broadcast};
use tokio::net::TcpListener;
use axum::{Json, extract::State, Router};
use tower::ServiceExt;

use asenix::api::mcp::handle_mcp;
use asenix::config::{Config, HubConfig, PheromoneConfig, TrustConfig, WorkersConfig, AcceptanceConfig};
use asenix::domain::agent::{AgentRegistration, AgentConfirmation};
use asenix::domain::atom::{AtomInput, AtomType, Provenance};
use asenix::state::AppState;

/// Test database configuration
pub const TEST_DATABASE_URL: &str = "postgres://asenix:asenix_password@localhost:5432/asenix_test";

/// Deterministic test configuration
pub fn create_test_config() -> Config {
    Config {
        hub: HubConfig {
            name: "test-hub".to_string(),
            domain: "test.asenix".to_string(),
            listen_address: "127.0.0.1:0".to_string(),
            embedding_endpoint: "http://localhost:11434".to_string(),
            embedding_model: "nomic-embed-text".to_string(),
            embedding_dimension: 384,
            structured_vector_reserved_dims: 10,
            dims_per_numeric_key: 2,
            dims_per_categorical_key: 4,
            neighbourhood_radius: 2.0,
            summary_llm_endpoint: Some("http://localhost:11434".to_string()),
            summary_llm_model: Some("nomic-embed-text".to_string()),
        },
        pheromone: PheromoneConfig {
            decay_half_life_hours: 24,
            attraction_cap: 10.0,
            novelty_radius: 1.0,
            disagreement_threshold: 0.5,
        },
        trust: TrustConfig {
            reliability_threshold: 0.7,
            independence_ancestry_depth: 3,
            probation_atom_count: 5,
            max_atoms_per_hour: 100,
        },
        workers: WorkersConfig {
            embedding_pool_size: 4,
            decay_interval_minutes: 60,
            claim_ttl_hours: 24,
            staleness_check_interval_minutes: 30,
            bounty_needed_novelty_threshold: 0.7,
            bounty_sparse_region_max_atoms: 3,
        },
        acceptance: AcceptanceConfig {
            required_provenance_fields: vec![
                "methodology".to_string(),
                "data_source".to_string(),
                "confidence".to_string(),
            ],
        },
    }
}

/// Reset database to clean state
pub async fn reset_database(pool: &PgPool) -> Result<(), sqlx::Error> {
    // Truncate all tables in correct order to respect foreign key constraints
    sqlx::query("TRUNCATE TABLE edges, atoms, agents, condition_registry CASCADE")
        .execute(pool)
        .await?;
    
    // Reset sequences
    sqlx::query("ALTER SEQUENCE agents_agent_id_seq RESTART WITH 1")
        .execute(pool)
        .await?;
    
    sqlx::query("ALTER SEQUENCE atoms_atom_id_seq RESTART WITH 1")
        .execute(pool)
        .await?;
    
    // Re-insert condition registry data
    sqlx::query(r#"
        INSERT INTO condition_registry (key, value_type, description) VALUES
        ('temperature', 'numeric', 'Temperature measurement in Celsius'),
        ('pressure', 'numeric', 'Pressure measurement in kPa'),
        ('location', 'categorical', 'Geographic location identifier'),
        ('experiment_type', 'categorical', 'Type of experimental procedure')
    "#).execute(pool).await?;
    
    Ok(())
}

/// Create test application with clean database
pub async fn create_test_app() -> Result<(Arc<AppState>, Router), anyhow::Error> {
    let config = create_test_config();
    
    // Connect to database
    let pool = PgPool::connect(TEST_DATABASE_URL).await?;
    
    // Reset database to clean state
    reset_database(&pool).await?;
    
    // Run migrations
    sqlx::migrate!("./migrations").run(&pool).await?;
    
    // Create channels
    let (embedding_queue_tx, _embedding_queue_rx) = mpsc::channel(100);
    let (sse_broadcast_tx, _sse_broadcast_rx) = broadcast::channel(100);
    
    // Create app state
    let state = Arc::new(AppState::new(pool, config, embedding_queue_tx, sse_broadcast_tx).await?);
    
    // Create router
    let app = Router::new()
        .route("/mcp", axum::routing::post(handle_mcp))
        .route("/health", axum::routing::get(asenix::api::handlers::health_check))
        .route("/metrics", axum::routing::get(asenix::api::handlers::metrics))
        .route("/review", axum::routing::get(asenix::api::handlers::get_review_queue))
        .route("/review/:id", axum::routing::post(asenix::api::handlers::review_atom))
        .with_state(state.clone());
    
    Ok((state, app))
}

/// Test agent with signing capability
#[derive(Clone)]
pub struct TestAgent {
    pub agent_id: String,
    pub keypair: Keypair,
    pub public_key: String,
}

impl TestAgent {
    /// Create a new test agent
    pub fn new() -> Self {
        let mut csprng = rand::rngs::OsRng;
        let keypair = Keypair::generate(&mut csprng);
        let public_key = hex::encode(keypair.public.to_bytes());
        
        Self {
            agent_id: String::new(), // Will be set after registration
            keypair,
            public_key,
        }
    }
    
    /// Register the agent and return updated TestAgent with agent_id
    pub async fn register(self, app: &Router) -> Result<Self, anyhow::Error> {
        let request = axum::http::Request::builder()
            .method("POST")
            .uri("/mcp")
            .header("content-type", "application/json")
            .body(axum::body::Body::from(json!({
                "jsonrpc": "2.0",
                "method": "register_agent",
                "params": {
                    "public_key": self.public_key
                },
                "id": "register"
            }).to_string()))?;
        
        let response = app.clone().oneshot(request).await?;
        let (_, body) = response.into_parts();
        
        let response_body: Value = serde_json::from_slice(&axum::body::to_bytes(body, usize::MAX).await?)?;
        
        let agent_id = response_body["result"]["agent_id"].as_str()
            .ok_or_else(|| anyhow::anyhow!("No agent_id in response"))?
            .to_string();
        
        Ok(TestAgent {
            agent_id,
            keypair: self.keypair,
            public_key: self.public_key,
        })
    }
    
    /// Confirm the agent registration
    pub async fn confirm(&self, app: &Router) -> Result<String, anyhow::Error> {
        // First get the challenge
        let request = axum::http::Request::builder()
            .method("POST")
            .uri("/mcp")
            .header("content-type", "application/json")
            .body(axum::body::Body::from(json!({
                "jsonrpc": "2.0",
                "method": "confirm_agent",
                "params": {
                    "agent_id": self.agent_id,
                    "signature": "dummy_signature"
                },
                "id": "confirm"
            }).to_string()))?;
        
        let response = app.clone().oneshot(request).await?;
        let (_, body) = response.into_parts();
        
        let response_body: Value = serde_json::from_slice(&axum::body::to_bytes(body, usize::MAX).await?)?;
        
        // If we get an authentication error, we need to get the challenge first
        if response_body["error"]["code"].as_i64() == Some(-32001) {
            // Get agent info to retrieve challenge
            let pool = &app.state().pool; // This would need access to state
            // For now, let's assume we need to sign a dummy challenge
            let challenge = "test_challenge";
            let challenge_bytes = challenge.as_bytes();
            let signature = self.keypair.sign(challenge_bytes);
            let signature_hex = hex::encode(signature.to_bytes());
            
            let request = axum::http::Request::builder()
                .method("POST")
                .uri("/mcp")
                .header("content-type", "application/json")
                .body(axum::body::Body::from(json!({
                    "jsonrpc": "2.0",
                    "method": "confirm_agent",
                    "params": {
                        "agent_id": self.agent_id,
                        "signature": signature_hex
                    },
                    "id": "confirm"
                }).to_string()))?;
            
            let response = app.clone().oneshot(request).await?;
            let (_, body) = response.into_parts();
            
            let response_body: Value = serde_json::from_slice(&axum::body::to_bytes(body, usize::MAX).await?)?;
            
            if response_body["error"].is_null() {
                return Ok("confirmed".to_string());
            } else {
                return Err(anyhow::anyhow!("Confirmation failed: {}", response_body["error"]));
            }
        }
        
        Ok("confirmed".to_string())
    }
    
    /// Sign parameters for MCP requests
    pub fn sign_params(&self, mut params: Value) -> Result<String, anyhow::Error> {
        // Remove signature field if present
        if let Some(obj) = params.as_object_mut() {
            obj.remove("signature");
        }
        
        // Create canonical JSON string
        let canonical = serde_json::to_string(&params)?;
        
        // Sign the canonical string
        let signature = self.keypair.sign(canonical.as_bytes());
        let signature_hex = hex::encode(signature.to_bytes());
        
        Ok(signature_hex)
    }
    
    /// Create signed MCP request parameters
    pub fn create_signed_params(&self, mut params: Value) -> Result<Value, anyhow::Error> {
        let signature = self.sign_params(params.clone())?;
        
        if let Some(obj) = params.as_object_mut() {
            obj.insert("signature".to_string(), json!(signature));
            obj.insert("agent_id".to_string(), json!(self.agent_id));
        }
        
        Ok(params)
    }
}

/// Create canonical atom input for testing
pub fn create_test_atom_input(
    atom_type: AtomType,
    domain: &str,
    statement: &str,
    conditions: Value,
    provenance: Value,
) -> AtomInput {
    AtomInput {
        atom_type,
        domain: domain.to_string(),
        project_id: None,
        statement: statement.to_string(),
        conditions,
        metrics: None,
        provenance,
        signature: vec![], // Will be filled during signing
        artifact_tree_hash: None,
        artifact_inline: None,
    }
}

/// Helper to build test graphs
pub struct GraphBuilder {
    atoms: Vec<AtomInput>,
    edges: Vec<(String, String, String)>, // (source, target, edge_type)
}

impl GraphBuilder {
    pub fn new() -> Self {
        Self {
            atoms: Vec::new(),
            edges: Vec::new(),
        }
    }
    
    pub fn add_atom(mut self, atom: AtomInput) -> Self {
        self.atoms.push(atom);
        self
    }
    
    pub fn add_edge(mut self, source: &str, target: &str, edge_type: &str) -> Self {
        self.edges.push((source.to_string(), target.to_string(), edge_type.to_string()));
        self
    }
    
    pub async fn build(self, agent: &TestAgent, app: &Router) -> Result<Vec<String>, anyhow::Error> {
        let mut atom_ids = Vec::new();
        
        // Publish atoms
        for atom in self.atoms {
            let params = agent.create_signed_params(json!({
                "atoms": [json!({
                    "atom_type": match atom.atom_type {
                        AtomType::Hypothesis => "hypothesis",
                        AtomType::Finding => "finding",
                        AtomType::NegativeResult => "negative_result",
                        AtomType::Delta => "delta",
                        AtomType::ExperimentLog => "experiment_log",
                        AtomType::Synthesis => "synthesis",
                        AtomType::Bounty => "bounty",
                    },
                    "domain": atom.domain,
                    "statement": atom.statement,
                    "conditions": atom.conditions,
                    "provenance": atom.provenance,
                    "signature": hex::encode(atom.signature)
                })]
            }))?;
            
            let request = axum::http::Request::builder()
                .method("POST")
                .uri("/mcp")
                .header("content-type", "application/json")
                .body(axum::body::Body::from(json!({
                    "jsonrpc": "2.0",
                    "method": "publish_atoms",
                    "params": params,
                    "id": "publish"
                }).to_string()))?;
            
            let response = app.clone().oneshot(request).await?;
            let (_, body) = response.into_parts();
            
            let response_body: Value = serde_json::from_slice(&axum::body::to_bytes(body, usize::MAX).await?)?;
            
            if response_body["error"].is_null() {
                let published = response_body["result"]["published_atoms"].as_array()
                    .ok_or_else(|| anyhow::anyhow!("No published_atoms in response"))?;
                
                for atom_id in published {
                    atom_ids.push(atom_id.as_str().unwrap().to_string());
                }
            } else {
                return Err(anyhow::anyhow!("Publish failed: {}", response_body["error"]));
            }
        }
        
        // TODO: Add edges when edge publishing is implemented
        
        Ok(atom_ids)
    }
}

/// Verify graph consistency between database and cache
pub async fn verify_graph_consistency(state: &Arc<AppState>) -> Result<(), anyhow::Error> {
    let cache = state.graph_cache.read().await;
    
    // Count atoms in database
    let db_atom_count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM atoms")
        .fetch_one(&state.pool)
        .await?;
    
    // Count nodes in cache
    let cache_node_count = cache.node_count();
    
    if db_atom_count as usize != cache_node_count {
        return Err(anyhow::anyhow!(
            "Atom count mismatch: DB={}, Cache={}",
            db_atom_count,
            cache_node_count
        ));
    }
    
    // Count edges in database
    let db_edge_count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM edges")
        .fetch_one(&state.pool)
        .await?;
    
    // Count edges in cache
    let cache_edge_count = cache.edge_count();
    
    if db_edge_count as usize != cache_edge_count {
        return Err(anyhow::anyhow!(
            "Edge count mismatch: DB={}, Cache={}",
            db_edge_count,
            cache_edge_count
        ));
    }
    
    Ok(())
}

/// Poll for embedding status with timeout
pub async fn poll_embedding_status(
    atom_id: &str,
    state: &Arc<AppState>,
    max_wait_ms: u64,
) -> Result<(), anyhow::Error> {
    let start = std::time::Instant::now();
    
    while start.elapsed().as_millis() < max_wait_ms {
        let status: Option<String> = sqlx::query_scalar(
            "SELECT embedding_status FROM atoms WHERE atom_id = $1"
        )
        .bind(atom_id)
        .fetch_optional(&state.pool)
        .await?;
        
        if let Some(status) = status {
            if status == "ready" {
                return Ok(());
            }
        }
        
        tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;
    }
    
    Err(anyhow::anyhow!("Embedding not ready within {}ms", max_wait_ms))
}

/// Concurrency testing helper
pub struct ConcurrencyTester {
    handles: Vec<tokio::task::JoinHandle<Result<(), anyhow::Error>>>,
}

impl ConcurrencyTester {
    pub fn new() -> Self {
        Self {
            handles: Vec::new(),
        }
    }
    
    pub fn add_task<F>(&mut self, future: F) 
    where 
        F: std::future::Future<Output = Result<(), anyhow::Error>> + Send + 'static,
    {
        self.handles.push(tokio::spawn(future));
    }
    
    pub async fn run_with_timeout(self, timeout_ms: u64) -> Result<Vec<Result<(), anyhow::Error>>, anyhow::Error> {
        let results = tokio::time::timeout(
            tokio::time::Duration::from_millis(timeout_ms),
            futures::future::join_all(self.handles)
        ).await??;
        
        Ok(results)
    }
}

// ===== PHASE 4 EMBEDDING TEST HELPERS =====

/// Mock embedding server for testing
pub struct MockEmbeddingServer {
    pub url: String,
    pub handle: tokio::task::JoinHandle<()>,
    delay_ms: u64,
    failure_counter: Arc<AtomicU32>,
}

impl MockEmbeddingServer {
    /// Create a new mock embedding server
    pub async fn new(embedding_dim: usize) -> Result<Self, anyhow::Error> {
        Self::new_with_config(embedding_dim, 0, Arc::new(AtomicU32::new(0))).await
    }

    /// Create a mock server with custom delay and failure counter
    pub async fn new_with_config(
        embedding_dim: usize, 
        delay_ms: u64,
        failure_counter: Arc<AtomicU32>
    ) -> Result<Self, anyhow::Error> {
        let listener = TcpListener::bind("127.0.0.1:0").await?;
        let port = listener.local_addr()?.port();
        let url = format!("http://127.0.0.1:{}", port);

        let failure_counter_clone = failure_counter.clone();
        let handle = tokio::spawn(async move {
            let app = Router::new()
                .route("/v1/embeddings", axum::routing::post(move |Json(payload): Json<serde_json::Value>| async move {
                    // Check if we should fail for this input
                    let input = payload["input"].as_str().unwrap_or("");
                    let current_count = failure_counter_clone.load(Ordering::SeqCst);
                    
                    if input.contains("fail_me") && current_count < 3 {
                        failure_counter_clone.fetch_add(1, Ordering::SeqCst);
                        return axum::response::Response::builder()
                            .status(500)
                            .body(axum::body::Body::from("Internal server error"))
                            .unwrap();
                    }

                    // Add delay if configured
                    if delay_ms > 0 {
                        tokio::time::sleep(tokio::time::Duration::from_millis(delay_ms)).await;
                    }

                    // Generate deterministic vector from input hash
                    let mut hash = std::collections::hash_map::DefaultHasher::new();
                    hash.write(input.as_bytes());
                    let hash_value = hash.finish();

                    let mut embedding = Vec::with_capacity(embedding_dim);
                    for i in 0..embedding_dim {
                        let value = ((hash_value >> (i % 64)) as f32) / (u64::MAX as f32) * 2.0 - 1.0;
                        embedding.push(value);
                    }

                    let response = json!({
                        "object": "list",
                        "data": [{
                            "object": "embedding",
                            "embedding": embedding,
                            "index": 0
                        }],
                        "model": "test-model",
                        "usage": {
                            "prompt_tokens": input.len(),
                            "total_tokens": input.len()
                        }
                    });

                    Json(response)
                }));

            axum::serve(listener, app).await.unwrap();
        });

        // Give the server a moment to start
        tokio::time::sleep(tokio::time::Duration::from_millis(50)).await;

        Ok(Self {
            url,
            handle,
            delay_ms,
            failure_counter,
        })
    }

    /// Shutdown the server
    pub async fn shutdown(self) {
        self.handle.abort();
    }
}

/// Wait for embedding to be ready
pub async fn wait_for_embedding(
    atom_id: &str,
    pool: &PgPool,
) -> Result<serde_json::Value, anyhow::Error> {
    let start = std::time::Instant::now();
    
    while start.elapsed().as_millis() < 5000 {
        let row = sqlx::query!(
            "SELECT embedding, embedding_status FROM atoms WHERE atom_id = $1",
            atom_id
        )
        .fetch_optional(pool)
        .await?;
        
        if let Some(row) = row {
            if row.embedding_status == "ready" {
                return Ok(row.embedding.unwrap_or(serde_json::Value::Null));
            }
        }
        
        tokio::time::sleep(tokio::time::Duration::from_millis(50)).await;
    }
    
    Err(anyhow::anyhow!("Embedding not ready within 5 seconds"))
}

/// Create atom input with sensible defaults
pub fn make_atom_input(
    atom_type: Option<AtomType>,
    statement: Option<&str>,
    conditions: Option<Value>,
    metrics: Option<Value>,
    provenance: Option<Value>,
) -> AtomInput {
    AtomInput {
        atom_type: atom_type.unwrap_or(AtomType::Finding),
        domain: "test".to_string(),
        project_id: None,
        statement: statement.unwrap_or("Test statement").to_string(),
        conditions: conditions.unwrap_or_else(|| json!({})),
        metrics,
        provenance: provenance.unwrap_or_else(|| json!({
            "methodology": "test",
            "data_source": "test",
            "confidence": 0.5
        })),
        signature: vec![], // Will be filled during signing
        artifact_tree_hash: None,
        artifact_inline: None,
    }
}

/// Seed condition registry with test data
pub async fn seed_condition_registry(pool: &PgPool) -> Result<(), anyhow::Error> {
    // Clear existing registry
    sqlx::query("DELETE FROM condition_registry").execute(pool).await?;

    // Insert test conditions
    let conditions = vec![
        ("test", "model_params", "float", Some("count"), true),
        ("test", "dataset", "string", None, true),
        ("test", "architecture", "string", None, false),
        ("test", "learning_rate", "float", Some("rate"), false),
    ];

    for (domain, key_name, value_type, unit, required) in conditions {
        sqlx::query!(
            r#"
            INSERT INTO condition_registry (domain, key_name, value_type, unit, required)
            VALUES ($1, $2, $3, $4, $5)
            "#,
            domain,
            key_name,
            value_type,
            unit,
            required
        )
        .execute(pool)
        .await?;
    }

    Ok(())
}
