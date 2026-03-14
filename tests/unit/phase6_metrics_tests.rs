use crate::api::handlers::{metrics, Metrics};
use crate::state::{AppState, SseEvent};
use crate::config::Config;
use sqlx::PgPool;
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};
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
async fn test_prometheus_metrics_format() {
    let state = setup_test_state().await;
    
    // Create test metrics
    let metrics = Metrics {
        publish_requests_accepted: AtomicU64::new(100),
        publish_requests_rejected: AtomicU64::new(10),
        publish_requests_queued: AtomicU64::new(250),
        rate_limit_rejections: AtomicU64::new(5),
        embedding_jobs_completed: AtomicU64::new(200),
        embedding_jobs_failed: AtomicU64::new(2),
        contradictions_detected: AtomicU64::new(3),
    };
    
    // Format metrics
    let output = metrics.format_prometheus(&state).await
        .expect("Should format metrics successfully");
    
    // Verify Prometheus format
    assert!(output.contains("# HELP asenix_publish_requests_total Total number of publish requests"));
    assert!(output.contains("# TYPE asenix_publish_requests_total counter"));
    assert!(output.contains("asenix_publish_requests_total{status=\"accepted\"} 100"));
    assert!(output.contains("asenix_publish_requests_total{status=\"rejected\"} 10"));
    assert!(output.contains("asenix_publish_requests_total{status=\"queued\"} 250"));
    
    assert!(output.contains("# HELP mote_rate_limit_rejections_total Total number of rate limit rejections"));
    assert!(output.contains("mote_rate_limit_rejections_total 5"));
    
    assert!(output.contains("# HELP mote_embedding_jobs_total Total number of embedding jobs"));
    assert!(output.contains("mote_embedding_jobs_total{status=\"completed\"} 200"));
    assert!(output.contains("mote_embedding_jobs_total{status=\"failed\"} 2"));
    
    assert!(output.contains("# HELP mote_contradictions_detected_total Total number of contradictions detected"));
    assert!(output.contains("mote_contradictions_detected_total 3"));
}

#[tokio::test]
async fn test_metrics_endpoint_response() {
    let state = setup_test_state().await;
    
    // Call metrics endpoint
    let response = metrics(State(state)).await;
    
    assert!(response.is_ok(), "Metrics endpoint should succeed");
    
    let output = response.unwrap();
    assert!(output.contains("# HELP"), "Response should contain Prometheus HELP comments");
    assert!(output.contains("# TYPE"), "Response should contain Prometheus TYPE comments");
}

#[tokio::test]
async fn test_metrics_counter_increments() {
    let metrics = Metrics::default();
    
    // Test counter increments
    metrics.publish_requests_accepted.fetch_add(1, Ordering::Relaxed);
    metrics.rate_limit_rejections.fetch_add(1, Ordering::Relaxed);
    
    assert_eq!(metrics.publish_requests_accepted.load(Ordering::Relaxed), 1);
    assert_eq!(metrics.rate_limit_rejections.load(Ordering::Relaxed), 1);
}

#[tokio::test]
async fn test_database_gauges() {
    let state = setup_test_state().await;
    
    // Insert test data
    sqlx::query("INSERT INTO agents (agent_id, public_key, confirmed) VALUES ('test-agent-1', 'key1', TRUE)")
        .execute(&state.pool)
        .await
        .expect("Failed to insert confirmed agent");
    
    sqlx::query("INSERT INTO agents (agent_id, public_key, confirmed) VALUES ('test-agent-2', 'key2', FALSE)")
        .execute(&state.pool)
        .await
        .expect("Failed to insert unconfirmed agent");
    
    sqlx::query("INSERT INTO atoms (atom_id, type, domain, statement, lifecycle, embedding_status) 
                VALUES ('test-atom-1', 'hypothesis', 'test-domain', 'Test statement', 'published', 'ready')")
        .execute(&state.pool)
        .await
        .expect("Failed to insert atom");
    
    sqlx::query("INSERT INTO claims (claim_id, agent_id, atom_id, active) VALUES ('test-claim-1', 'test-agent-1', 'test-atom-1', TRUE)")
        .execute(&state.pool)
        .await
        .expect("Failed to insert claim");
    
    // Format metrics
    let metrics = Metrics::default();
    let output = metrics.format_prometheus(&state).await
        .expect("Should format metrics successfully");
    
    // Verify database gauges are included
    assert!(output.contains("mote_agents_total"), "Should include agents total");
    assert!(output.contains("mote_atoms_total"), "Should include atoms total");
    assert!(output.contains("mote_claims_active"), "Should include active claims");
    
    // Clean up
    sqlx::query("DELETE FROM claims WHERE claim_id = 'test-claim-1'")
        .execute(&state.pool)
        .await
        .expect("Failed to clean up claim");
    
    sqlx::query("DELETE FROM atoms WHERE atom_id = 'test-atom-1'")
        .execute(&state.pool)
        .await
        .expect("Failed to clean up atom");
    
    sqlx::query("DELETE FROM agents WHERE agent_id IN ('test-agent-1', 'test-agent-2')")
        .execute(&state.pool)
        .await
        .expect("Failed to clean up agents");
}

#[tokio::test]
async fn test_in_memory_gauges() {
    let state = setup_test_state().await;
    
    // Add some data to graph cache
    {
        let mut cache = state.graph_cache.write().await;
        cache.add_node("test-node-1".to_string());
        cache.add_node("test-node-2".to_string());
        cache.add_edge("test-node-1".to_string(), "test-node-2".to_string(), "supports");
    }
    
    // Format metrics
    let metrics = Metrics::default();
    let output = metrics.format_prometheus(&state).await
        .expect("Should format metrics successfully");
    
    // Verify in-memory gauges
    assert!(output.contains("mote_graph_cache_nodes"), "Should include graph cache nodes");
    assert!(output.contains("mote_graph_cache_edges"), "Should include graph cache edges");
    assert!(output.contains("mote_graph_cache_nodes 2"), "Should have 2 nodes");
    assert!(output.contains("mote_graph_cache_edges 1"), "Should have 1 edge");
}

#[tokio::test]
async fn test_metrics_error_handling() {
    let state = setup_test_state().await;
    
    // Simulate database error by dropping the pool
    drop(state.pool.clone());
    
    let metrics = Metrics::default();
    let result = metrics.format_prometheus(&state).await;
    
    // Should handle database errors gracefully
    assert!(result.is_err(), "Should return error when database is unavailable");
}
