use crate::api::sse::{sse_events, TypedSseEvent, SseQueryParams};
use crate::state::{AppState, SseEvent};
use axum::extract::Query;
use axum::response::Response;
use serde_json::json;
use sqlx::PgPool;
use std::sync::Arc;
use tokio::sync::{broadcast, mpsc};
use uuid::Uuid;

async fn setup_test_state() -> (Arc<AppState>, PgPool) {
    // This would set up a test database and state
    // For now, we'll create a minimal setup
    let config = crate::config::Config::default();
    let pool = sqlx::PgPool::connect("postgresql://test:test@localhost/test_db")
        .await
        .expect("Failed to connect to test database");
    
    let (embedding_tx, _) = tokio::sync::mpsc::channel(100);
    let (sse_tx, _) = tokio::sync::broadcast::channel(100);
    
    let state = AppState::new(pool.clone(), Arc::new(config), embedding_tx, sse_tx)
        .await
        .expect("Failed to create test state");
    
    (Arc::new(state), pool)
}

#[tokio::test]
async fn test_sse_event_subscription() {
    let (state, _pool) = setup_test_state().await;
    
    // Test valid subscription
    let query = SseQueryParams {
        region: vec![0.1, 0.2, 0.3],
        radius: Some(0.5),
        types: Some(vec!["atom_published".to_string()]),
    };
    
    let response = sse_events(Query(query), State(state))
        .await;
    
    assert!(response.is_ok(), "SSE subscription should succeed");
}

#[tokio::test]
async fn test_sse_event_filtering() {
    let (state, _pool) = setup_test_state().await;
    
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
    let query = SseQueryParams {
        region: vec![0.1, 0.2, 0.3],
        radius: Some(0.5),
        types: Some(vec!["atom_published".to_string()]),
    };
    
    let response = sse_events(Query(query), State(state))
        .await;
    
    assert!(response.is_ok(), "SSE filtering should work");
}

#[tokio::test]
async fn test_sse_invalid_radius() {
    let (state, _pool) = setup_test_state().await;
    
    // Test invalid radius (negative)
    let query = SseQueryParams {
        region: vec![0.1, 0.2, 0.3],
        radius: Some(-0.5),
        types: None,
    };
    
    let response = sse_events(Query(query), State(state))
        .await;
    
    assert!(response.is_err(), "Invalid radius should return error");
}

#[tokio::test]
async fn test_sse_invalid_region_dimension() {
    let (state, _pool) = setup_test_state().await;
    
    // Test invalid region dimension (not 1536)
    let query = SseQueryParams {
        region: vec![0.1, 0.2], // Wrong dimension
        radius: Some(0.5),
        types: None,
    };
    
    let response = sse_events(Query(query), State(state))
        .await;
    
    assert!(response.is_err(), "Invalid region dimension should return error");
}

#[tokio::test]
async fn test_cosine_distance_calculation() {
    use crate::api::sse::cosine_distance;
    
    let a = vec![1.0, 0.0, 0.0];
    let b = vec![0.0, 1.0, 0.0];
    let c = vec![1.0, 0.0, 0.0];
    
    // Orthogonal vectors should have distance 1.0
    let distance_ab = cosine_distance(&a, &b);
    assert!((distance_ab - 1.0).abs() < f64::EPSILON);
    
    // Identical vectors should have distance 0.0
    let distance_ac = cosine_distance(&a, &c);
    assert!((distance_ac - 0.0).abs() < f64::EPSILON);
}

#[tokio::test]
async fn test_sse_event_serialization() {
    use crate::api::sse::format_sse_event;
    
    let event = TypedSseEvent::AtomPublished {
        atom_id: "test-atom-1".to_string(),
        atom_type: "hypothesis".to_string(),
        domain: "test-domain".to_string(),
    };
    
    let formatted = format_sse_event(&event);
    assert!(formatted.contains("event: atom_published"));
    assert!(formatted.contains("data: {\"atom_id\":\"test-atom-1\""));
}
