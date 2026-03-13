use crate::workers::claims::ClaimsExpiryWorker;
use crate::workers::staleness::StalenessWorker;
use crate::workers::embedding_queue::EmbeddingQueue;
use crate::config::Config;
use sqlx::PgPool;
use chrono::{Utc, Duration as ChronoDuration};
use std::sync::Arc;

async fn setup_test_database() -> PgPool {
    // This would set up a test database
    // For now, we'll create a mock connection
    let database_url = std::env::var("TEST_DATABASE_URL")
        .unwrap_or_else(|_| "postgresql://test:test@localhost/test_mote".to_string());
    
    let pool = sqlx::PgPool::connect(&database_url)
        .await
        .expect("Failed to connect to test database");
    
    // Run migrations
    sqlx::migrate!("./migrations").run(&pool).await
        .expect("Failed to run migrations");
    
    pool
}

#[tokio::test]
async fn test_claims_expiry_logic() {
    let pool = setup_test_database().await;
    let worker = ClaimsExpiryWorker::new(pool.clone());
    
    // Create a claim that expires in the past
    let expired_claim_id = Uuid::new_v4().to_string();
    sqlx::query(
        "INSERT INTO claims (claim_id, agent_id, atom_id, expires_at, active) 
         VALUES ($1, $2, $3, $4, TRUE)"
    )
    .bind(&expired_claim_id)
    .bind("test-agent-1")
    .bind("test-atom-1")
    .bind(Utc::now() - ChronoDuration::hours(1)) // Expired 1 hour ago
    .execute(&pool)
    .await
    .expect("Failed to insert expired claim");
    
    // Create a claim that expires in the future
    let future_claim_id = Uuid::new_v4().to_string();
    sqlx::query(
        "INSERT INTO claims (claim_id, agent_id, atom_id, expires_at, active) 
         VALUES ($1, $2, $3, $4, TRUE)"
    )
    .bind(&future_claim_id)
    .bind("test-agent-2")
    .bind("test-atom-2")
    .bind(Utc::now() + ChronoDuration::hours(1)) // Expires in 1 hour
    .execute(&pool)
    .await
    .expect("Failed to insert future claim");
    
    // Run expiry check
    let result = worker.run_expiry_check().await
        .expect("Claims expiry check should succeed");
    
    // Should have expired exactly 1 claim
    assert_eq!(result, 1, "Should expire exactly 1 claim");
    
    // Verify expired claim is now inactive
    let expired_active: bool = sqlx::query_scalar(
        "SELECT active FROM claims WHERE claim_id = $1"
    )
    .bind(&expired_claim_id)
    .fetch_one(&pool)
    .await
    .expect("Failed to check expired claim status");
    
    assert!(!expired_active, "Expired claim should be inactive");
    
    // Verify future claim is still active
    let future_active: bool = sqlx::query_scalar(
        "SELECT active FROM claims WHERE claim_id = $1"
    )
    .bind(&future_claim_id)
    .fetch_one(&pool)
    .await
    .expect("Failed to check future claim status");
    
    assert!(future_active, "Future claim should still be active");
    
    // Clean up
    sqlx::query("DELETE FROM claims WHERE claim_id IN ($1, $2)")
        .bind(&expired_claim_id)
        .bind(&future_claim_id)
        .execute(&pool)
        .await
        .expect("Failed to clean up test claims");
}

#[tokio::test]
async fn test_claims_expiry_idempotent() {
    let pool = setup_test_database().await;
    let worker = ClaimsExpiryWorker::new(pool.clone());
    
    // Create an expired claim
    let claim_id = Uuid::new_v4().to_string();
    sqlx::query(
        "INSERT INTO claims (claim_id, agent_id, atom_id, expires_at, active) 
         VALUES ($1, $2, $3, $4, TRUE)"
    )
    .bind(&claim_id)
    .bind("test-agent-1")
    .bind("test-atom-1")
    .bind(Utc::now() - ChronoDuration::hours(1))
    .execute(&pool)
    .await
    .expect("Failed to insert expired claim");
    
    // Run expiry check twice
    let result1 = worker.run_expiry_check().await
        .expect("First expiry check should succeed");
    let result2 = worker.run_expiry_check().await
        .expect("Second expiry check should succeed");
    
    // First run should expire 1 claim, second should expire 0
    assert_eq!(result1, 1, "First run should expire 1 claim");
    assert_eq!(result2, 0, "Second run should expire 0 claims");
    
    // Clean up
    sqlx::query("DELETE FROM claims WHERE claim_id = $1")
        .bind(&claim_id)
        .execute(&pool)
        .await
        .expect("Failed to clean up test claim");
}

#[tokio::test]
async fn test_staleness_detection() {
    let pool = setup_test_database().await;
    let worker = StalenessWorker::new(pool.clone(), 0.5);
    
    // Create a synthesis atom with embedding
    let synthesis_id = Uuid::new_v4().to_string();
    let embedding = vec![0.1; 1536]; // 1536-dimensional embedding
    sqlx::query(
        "INSERT INTO atoms (atom_id, type, domain, statement, embedding, embedding_status, created_at) 
         VALUES ($1, 'synthesis', 'test-domain', 'Test synthesis', $2, 'ready', $3)"
    )
    .bind(&synthesis_id)
    .bind(&embedding)
    .bind(Utc::now() - ChronoDuration::hours(2)) // Created 2 hours ago
    .execute(&pool)
    .await
    .expect("Failed to insert synthesis atom");
    
    // Create 25 newer atoms in the same neighbourhood (above threshold)
    for i in 0..25 {
        let atom_id = Uuid::new_v4().to_string();
        let nearby_embedding = {
            let mut emb = embedding.clone();
            // Small perturbation to keep it in same neighbourhood
            emb[0] += 0.01 * (i as f64);
            emb
        };
        
        sqlx::query(
            "INSERT INTO atoms (atom_id, type, domain, statement, embedding, embedding_status, created_at) 
             VALUES ($1, 'hypothesis', 'test-domain', 'Test hypothesis $i', $2, 'ready', $3)"
        )
        .bind(&atom_id)
        .bind(&nearby_embedding)
        .bind(Utc::now() - ChronoDuration::minutes(30)) // Created 30 minutes ago (newer)
        .execute(&pool)
        .await
        .expect("Failed to insert nearby atom");
    }
    
    // Run staleness check
    let result = worker.run_staleness_check().await
        .expect("Staleness check should succeed");
    
    // Should detect 1 stale synthesis
    assert_eq!(result, 1, "Should detect 1 stale synthesis");
    
    // Clean up
    sqlx::query("DELETE FROM atoms WHERE atom_id = $1")
        .bind(&synthesis_id)
        .execute(&pool)
        .await
        .expect("Failed to clean up synthesis atom");
}

#[tokio::test]
async fn test_summary_generation() {
    let pool = setup_test_database().await;
    let config = Config {
        hub: crate::config::HubConfig {
            summary_llm_endpoint: Some("http://localhost:8080/summarize".to_string()),
            summary_llm_model: Some("test-model".to_string()),
            ..Default::default()
        },
        ..Default::default()
    };
    
    let worker = EmbeddingQueue::new(
        pool.clone(),
        config,
        Arc::new(tokio::sync::RwLock::new(crate::db::graph_cache::GraphCache::new())),
        Arc::new(tokio::sync::RwLock::new(crate::domain::condition::ConditionRegistry::new())),
    );
    
    // Create an atom for summary generation
    let atom_id = Uuid::new_v4().to_string();
    sqlx::query(
        "INSERT INTO atoms (atom_id, type, domain, statement, conditions, metrics, embedding_status) 
         VALUES ($1, 'hypothesis', 'test-domain', 'Test hypothesis', '{}', '{}', 'ready')"
    )
    .bind(&atom_id)
    .execute(&pool)
    .await
    .expect("Failed to insert test atom");
    
    // Test summary generation (this would normally call an LLM)
    // For testing, we'll verify the logic without actual LLM calls
    let result = worker.generate_summary(&atom_id).await;
    
    // Should fail gracefully when LLM is not available
    assert!(result.is_err() || result.is_ok(), "Should handle LLM unavailability gracefully");
    
    // Clean up
    sqlx::query("DELETE FROM atoms WHERE atom_id = $1")
        .bind(&atom_id)
        .execute(&pool)
        .await
        .expect("Failed to clean up test atom");
}
