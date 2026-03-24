use asenix::api::sse::{sse_events, TypedSseEvent};
use asenix::state::{AppState, SseEvent};
use axum::extract::{Query, State};
use serde_json::json;
use std::sync::Arc;
use std::env;

async fn setup_test_state() -> Arc<AppState> {
    let database_url = env::var("DATABASE_URL")
        .unwrap_or_else(|_| "postgres://mote:asenix_password@localhost:5432/asenix_test".to_string());

    let config = asenix::config::Config {
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
            max_artifact_blob_bytes: 1048576,
            max_artifact_storage_per_agent_bytes: 10485760,
        },
        pheromone: asenix::config::PheromoneConfig {
            decay_half_life_atoms: 24,
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
            frontier_diversity_k: 8,
        },
        acceptance: asenix::config::AcceptanceConfig {
            required_provenance_fields: vec![],
        },
        mcp: asenix::config::McpConfig {
            allowed_origins: vec!["http://localhost:3000".to_string()],
        },
    };
    let pool = sqlx::PgPool::connect(&database_url)
        .await
        .expect("Failed to connect to test database");
    
    let (sse_tx, _) = tokio::sync::broadcast::channel(100);
    let storage = Arc::new(asenix::storage::LocalStorage::new(std::path::PathBuf::from("./test_artifacts")));

    let (embedding_tx, _embedding_rx) = tokio::sync::mpsc::channel::<String>(100);
    let state = AppState::new(pool, Arc::new(config), sse_tx, storage, embedding_tx)
        .await
        .expect("Failed to create test state");
    
    Arc::new(state)
}

#[tokio::test]
async fn test_sse_event_subscription() {
    let state = setup_test_state().await;

    // Test valid subscription with spatial filter
    let query = asenix::api::sse::SseQueryParams {
        region: Some("0.1,0.2,0.3".to_string()),
        radius: Some(0.5),
        types: Some("atom_published".to_string()),
    };

    let response = sse_events(State(state), Query(query))
        .await;

    assert!(response.is_ok(), "SSE subscription should succeed");
}

#[tokio::test]
async fn test_sse_event_filtering() {
    let state = setup_test_state().await;
    
    // Create test events
    let test_event = SseEvent {
        event_type: "atom_published".to_string(),
        data: json!({
            "atom_id": "test-atom-1",
            "embedding": vec![0.1, 0.2, 0.3]
        }),
        timestamp: chrono::Utc::now(),
    };
    
    // Send event to broadcast channel
    let _ = state.sse_broadcast_tx.send(test_event);
    
    // Test that filtering works
    let query = asenix::api::sse::SseQueryParams {
        region: Some("0.1,0.2,0.3".to_string()),
        radius: Some(0.5),
        types: Some("atom_published".to_string()),
    };
    
    let response = sse_events(State(state), Query(query))
        .await;
    
    assert!(response.is_ok(), "SSE filtering should work");
}

#[tokio::test]
async fn test_sse_invalid_radius() {
    let state = setup_test_state().await;
    
    // Test invalid radius (negative)
    let query = asenix::api::sse::SseQueryParams {
        region: Some("0.1,0.2,0.3".to_string()),
        radius: Some(-0.5),
        types: Some("atom_published".to_string()),
    };
    
    let response = sse_events(State(state), Query(query))
        .await;
    
    assert!(response.is_err(), "Invalid radius should return error");
}

#[tokio::test]
async fn test_sse_invalid_region_dimension() {
    let state = setup_test_state().await;
    
    // Test invalid region dimension (empty string)
    let query = asenix::api::sse::SseQueryParams {
        region: Some("".to_string()), // Invalid region
        radius: Some(0.5),
        types: Some("atom_published".to_string()),
    };
    
    let response = sse_events(State(state), Query(query))
        .await;
    
    assert!(response.is_err(), "Invalid region dimension should return error");
}

#[tokio::test]
async fn test_typed_sse_event_serialization() {
    let event = TypedSseEvent::AtomPublished {
        atom_id: "test-atom-1".to_string(),
        atom_type: "hypothesis".to_string(),
        domain: "test-domain".to_string(),
        embedding: Some(vec![0.1, 0.2, 0.3]),
    };
    
    // Test that the event can be serialized to JSON
    let serialized = serde_json::to_string(&event).unwrap();
    assert!(serialized.contains("atom_published"));
    assert!(serialized.contains("test-atom-1"));
    
    // Test that it can be deserialized back
    let deserialized: TypedSseEvent = serde_json::from_str(&serialized).unwrap();
    match deserialized {
        TypedSseEvent::AtomPublished { atom_id, .. } => {
            assert_eq!(atom_id, "test-atom-1");
        }
        _ => panic!("Expected AtomPublished event"),
    }
}

#[tokio::test]
async fn test_sse_no_params_succeeds() {
    let state = setup_test_state().await;

    // All params absent — should open the stream without error
    let query = asenix::api::sse::SseQueryParams {
        region: None,
        radius: None,
        types: None,
    };

    let response = sse_events(State(state), Query(query)).await;
    assert!(response.is_ok(), "SSE subscription with no params should succeed");
}

#[tokio::test]
async fn test_sse_region_without_radius_rejected() {
    let state = setup_test_state().await;

    // region provided but radius absent — should return 400
    let query = asenix::api::sse::SseQueryParams {
        region: Some("0.1,0.2,0.3".to_string()),
        radius: None,
        types: Some("atom_published".to_string()),
    };

    let response = sse_events(State(state), Query(query)).await;
    assert!(response.is_err(), "Region without radius should be rejected");
    if let Err((status, _)) = response {
        assert_eq!(status, axum::http::StatusCode::BAD_REQUEST);
    }
}

#[tokio::test]
async fn test_sse_synthesis_needed_has_domain() {
    let event = TypedSseEvent::SynthesisNeeded {
        cluster_center: vec![0.1, 0.2, 0.3],
        atom_count: 25,
        domain: "machine_learning".to_string(),
    };

    let serialized = serde_json::to_string(&event).unwrap();
    assert!(serialized.contains("synthesis_needed"));
    assert!(serialized.contains("machine_learning"));

    let deserialized: TypedSseEvent = serde_json::from_str(&serialized).unwrap();
    match deserialized {
        TypedSseEvent::SynthesisNeeded { domain, atom_count, .. } => {
            assert_eq!(domain, "machine_learning");
            assert_eq!(atom_count, 25);
        }
        _ => panic!("Expected SynthesisNeeded event"),
    }
}

#[tokio::test]
async fn test_pheromone_shift_typed_event_serialization() {
    let event = TypedSseEvent::PheromoneShift {
        atom_id: "test-atom-123".to_string(),
        field: "attraction".to_string(),
        old_value: 0.5,
        new_value: 0.8,
    };
    
    // Test that the event can be serialized to JSON
    let serialized = serde_json::to_string(&event).unwrap();
    assert!(serialized.contains("pheromone_shift"));
    assert!(serialized.contains("test-atom-123"));
    
    // Test that it can be deserialized back
    let deserialized: TypedSseEvent = serde_json::from_str(&serialized).unwrap();
    match deserialized {
        TypedSseEvent::PheromoneShift { atom_id, field, old_value, new_value } => {
            assert_eq!(atom_id, "test-atom-123");
            assert_eq!(field, "attraction");
            assert_eq!(old_value, 0.5);
            assert_eq!(new_value, 0.8);
        }
        _ => panic!("Expected PheromoneShift event"),
    }
}
