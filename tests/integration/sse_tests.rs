//! Integration tests for SSE event emission

use serde_json::json;
use serial_test::serial;
use super::{initialize_session, make_tool_call};

// We need direct access to AppState to subscribe to the broadcast channel.
use std::sync::Arc;
use std::env;
use tokio::sync::{mpsc, broadcast};
use asenix::config::Config;
use asenix::state::{AppState, SseEvent};
use asenix::api;
use asenix::db::pool::create_pool;
use asenix::storage::LocalStorage;
use axum::Router;

/// Build a test app and return (router, broadcast_rx) so tests can listen to SSE events.
async fn setup_test_app_with_sse() -> (Router, broadcast::Receiver<SseEvent>) {
    let database_url = env::var("DATABASE_URL")
        .unwrap_or_else(|_| "postgres://asenix:asenix_password@localhost:5432/asenix_test".to_string());

    let config = Config {
        hub: asenix::config::HubConfig {
            name: "test-hub".to_string(),
            domain: "test.asenix".to_string(),
            listen_address: "127.0.0.1:8080".to_string(),
            embedding_endpoint: "http://localhost:11434".to_string(),
            embedding_model: "nomic-embed-text".to_string(),
            embedding_dimension: 384,
            structured_vector_reserved_dims: 10,
            dims_per_numeric_key: 2,
            dims_per_categorical_key: 1,
            neighbourhood_radius: 0.1,
            summary_llm_endpoint: Some("http://localhost:11434".to_string()),
            summary_llm_model: Some("llama2".to_string()),
            artifact_storage_path: "./test_artifacts".to_string(),
            max_artifact_blob_bytes: 1048576,
            max_artifact_storage_per_agent_bytes: 10485760,
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
            required_provenance_fields: vec![],
        },
        mcp: asenix::config::McpConfig {
            allowed_origins: vec!["http://localhost:3000".to_string()],
        },
    };

    let pool = create_pool(&config, &database_url).await.expect("pool");

    // Truncate for clean state
    let tables = ["edges", "synthesis", "bounties", "claims", "atoms", "agents", "condition_registry"];
    for t in tables {
        sqlx::query(&format!("TRUNCATE TABLE {} CASCADE", t))
            .execute(&pool).await.ok();
    }

    let (sse_tx, sse_rx) = broadcast::channel(1000);
    let storage = Arc::new(LocalStorage::new(std::path::PathBuf::from("./test_artifacts")));

    let (embedding_tx, _embedding_rx) = tokio::sync::mpsc::channel::<String>(100);
    let state = AppState::new(pool, Arc::new(config), sse_tx, storage, embedding_tx)
        .await.expect("state");

    let router = Router::new()
        .route("/mcp", axum::routing::post(api::mcp_server::handle_mcp_request)
            .delete(api::mcp_server::handle_mcp_delete))
        .layer(axum::extract::DefaultBodyLimit::max(10 * 1024 * 1024))
        .with_state(Arc::new(state));

    (router, sse_rx)
}

#[serial]
#[tokio::test]
async fn test_atom_published_sse_event_emitted() {
    let (app, mut sse_rx) = setup_test_app_with_sse().await;
    let sid = initialize_session(&app).await;

    // Register agent
    let reg = make_tool_call(&app, &sid, "register",
        json!({"agent_name": "sse-test-agent"}), json!(1)).await.unwrap();
    let agent_id = reg["result"]["agent_id"].as_str().unwrap().to_string();
    let api_token = reg["result"]["api_token"].as_str().unwrap().to_string();

    // Publish an atom
    let pub_resp = make_tool_call(&app, &sid, "publish", json!({
        "agent_id":  agent_id,
        "api_token": api_token,
        "atom_type": "hypothesis",
        "domain":    "sse_test_domain",
        "statement": "SSE events should be emitted on publish",
        "conditions": {},
        "provenance": {}
    }), json!(2)).await.unwrap();

    let atom_id = pub_resp["result"]["atom_id"].as_str().unwrap().to_string();

    // The event should already be in the channel (fire-and-forget, sync send)
    let event = sse_rx.try_recv().expect("atom_published event should be in channel");
    assert_eq!(event.event_type, "atom_published");
    assert_eq!(event.data["atom_id"].as_str().unwrap(), atom_id);
    assert_eq!(event.data["domain"].as_str().unwrap(), "sse_test_domain");
    assert_eq!(event.data["atom_type"].as_str().unwrap(), "hypothesis");
}

#[serial]
#[tokio::test]
async fn test_contradiction_detected_sse_event_emitted() {
    // Contradiction detection is now async (EmbeddingWorker), not synchronous with publish.
    // This test verifies that publishing two opposing findings both produce atom_published events
    // and that no contradiction_detected event fires at publish time (it fires later, async).
    let (app, mut sse_rx) = setup_test_app_with_sse().await;
    let sid = initialize_session(&app).await;

    let reg = make_tool_call(&app, &sid, "register",
        json!({"agent_name": "sse-contradiction-agent"}), json!(1)).await.unwrap();
    let agent_id = reg["result"]["agent_id"].as_str().unwrap().to_string();
    let api_token = reg["result"]["api_token"].as_str().unwrap().to_string();

    // First finding
    make_tool_call(&app, &sid, "publish", json!({
        "agent_id": agent_id, "api_token": api_token,
        "atom_type": "finding", "domain": "sse_contra_domain",
        "statement": "Compound A increases yield",
        "conditions": {"compound": "A", "temperature": 25},
        "metrics": [{"name": "yield", "value": 0.9, "direction": "higher_better"}],
        "provenance": {}
    }), json!(2)).await.unwrap();

    // Second finding — opposing direction
    make_tool_call(&app, &sid, "publish", json!({
        "agent_id": agent_id, "api_token": api_token,
        "atom_type": "finding", "domain": "sse_contra_domain",
        "statement": "Compound A decreases yield",
        "conditions": {"compound": "A", "temperature": 25},
        "metrics": [{"name": "yield", "value": 0.1, "direction": "lower_better"}],
        "provenance": {}
    }), json!(3)).await.unwrap();

    // Collect synchronously-emitted events
    let mut events = vec![];
    while let Ok(e) = sse_rx.try_recv() {
        events.push(e);
    }

    // Both publishes should emit atom_published; contradiction_detected comes from EmbeddingWorker (async)
    let published_count = events.iter().filter(|e| e.event_type == "atom_published").count();
    assert_eq!(published_count, 2, "both findings should emit atom_published; got: {:?}",
        events.iter().map(|e| &e.event_type).collect::<Vec<_>>());
    let has_contradiction = events.iter().any(|e| e.event_type == "contradiction_detected");
    assert!(!has_contradiction, "contradiction_detected should not fire at publish time (it's async via EmbeddingWorker)");
}

#[serial]
#[tokio::test]
async fn test_synthesis_needed_event_type_constructable() {
    // Verify SseEvent with synthesis_needed type can be constructed and serialized
    let event = SseEvent {
        event_type: "synthesis_needed".to_string(),
        data: json!({
            "type": "synthesis_needed",
            "cluster_center": [0.1, 0.2, 0.3],
            "atom_count": 25
        }),
        timestamp: chrono::Utc::now(),
    };
    assert_eq!(event.event_type, "synthesis_needed");
    assert_eq!(event.data["atom_count"].as_u64().unwrap(), 25);

    let serialized = serde_json::to_string(&event).unwrap();
    assert!(serialized.contains("synthesis_needed"));
}

#[serial]
#[tokio::test]
async fn test_synthesis_needed_sse_event_roundtrip() {
    // Verifies that a synthesis_needed SseEvent can be sent through a broadcast channel
    // and received intact. StalenessWorker was merged into BountyWorker (Plan 06);
    // the channel contract is unchanged.
    let (sse_tx, mut sse_rx) = tokio::sync::broadcast::channel(100);

    let test_event = asenix::state::SseEvent {
        event_type: "synthesis_needed".to_string(),
        data: serde_json::json!({
            "type": "synthesis_needed",
            "cluster_center": [0.1, 0.2, 0.3, 0.4, 0.5],
            "atom_count": 25
        }),
        timestamp: chrono::Utc::now(),
    };

    let _ = sse_tx.send(test_event);

    let mut found_event = false;
    while let Ok(event) = sse_rx.try_recv() {
        if event.event_type == "synthesis_needed" {
            found_event = true;
            assert_eq!(event.data["type"], "synthesis_needed");
            assert_eq!(event.data["atom_count"], 25);
            let cluster_center = event.data["cluster_center"].as_array();
            assert!(cluster_center.is_some());
            assert_eq!(cluster_center.unwrap().len(), 5);
            break;
        }
    }

    assert!(found_event, "Should receive synthesis_needed event through SSE channel");
}

#[serial]
#[tokio::test]
async fn test_pheromone_shift_event_type_constructable() {
    // Verify PheromoneShift event can be constructed and serialized
    let event = SseEvent {
        event_type: "pheromone_shift".to_string(),
        data: json!({
            "type": "pheromone_shift",
            "atom_id": "test-atom-123",
            "field": "attraction",
            "old_value": 0.5,
            "new_value": 0.8
        }),
        timestamp: chrono::Utc::now(),
    };
    assert_eq!(event.event_type, "pheromone_shift");
    assert_eq!(event.data["atom_id"], "test-atom-123");
    assert_eq!(event.data["field"], "attraction");
    assert_eq!(event.data["old_value"], 0.5);
    assert_eq!(event.data["new_value"], 0.8);

    let serialized = serde_json::to_string(&event).unwrap();
    assert!(serialized.contains("pheromone_shift"));
}
