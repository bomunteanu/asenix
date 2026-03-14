//! Unit tests for exploration mode functionality
//!
//! Tests the new exploration sampling logic and domain novelty queries.

use asenix::db::queries;
use serial_test::serial;
use sqlx::PgPool;
use std::env;

async fn setup_test_db() -> PgPool {
    let database_url = env::var("DATABASE_URL")
        .unwrap_or_else(|_| "postgres://mote:asenix_password@localhost:5432/asenix_test".to_string());

    let pool = sqlx::PgPool::connect(&database_url)
        .await
        .expect("Failed to connect to test database");

    // Clean up database before each test (atoms depends on agents via FK)
    sqlx::query("TRUNCATE TABLE atoms CASCADE")
        .execute(&pool)
        .await
        .expect("Failed to truncate atoms table");
    sqlx::query("TRUNCATE TABLE agents CASCADE")
        .execute(&pool)
        .await
        .expect("Failed to truncate agents table");

    // Insert a test agent to satisfy the FK on atoms.author_agent_id
    sqlx::query(
        "INSERT INTO agents (agent_id, public_key, confirmed, created_at) \
         VALUES ('test-agent', decode('deadbeefdeadbeef', 'hex'), true, NOW()) \
         ON CONFLICT (agent_id) DO NOTHING"
    )
    .execute(&pool)
    .await
    .expect("Failed to insert test agent");

    pool
}

#[serial]
#[tokio::test]
async fn test_query_nearest_atom_with_density_empty_db() {
    let pool = setup_test_db().await;
    
    // Test with empty database
    let vector = vec![0.1, 0.2, 0.3, 0.4];
    let radius = 0.5;
    
    let result = queries::query_nearest_atom_with_density(&pool, vector, radius)
        .await
        .expect("Query should succeed");
    
    assert!(result.0.is_none()); // No nearest atom
    assert_eq!(result.1, 0); // No atoms in radius
}

#[serial]
#[tokio::test]
async fn test_query_nearest_atom_with_density_with_data() {
    let pool = setup_test_db().await;

    // Build 384-dim embeddings: atom1 and atom2 close together, atom3 far away
    let mut emb1 = vec![0.0f32; 384];
    emb1[0] = 1.0;
    let mut emb2 = vec![0.0f32; 384];
    emb2[0] = 0.99;
    let mut emb3 = vec![0.0f32; 384];
    emb3[383] = 1.0; // orthogonal — far from emb1/emb2

    let atoms: Vec<(&str, &str, &str, Vec<f32>)> = vec![
        ("atom1", "test", "Statement 1", emb1),
        ("atom2", "test", "Statement 2", emb2),
        ("atom3", "test", "Statement 3", emb3),
    ];

    for (id, domain, statement, embedding) in &atoms {
        let pg_vector = pgvector::Vector::from(embedding.clone());

        sqlx::query(
            r#"
            INSERT INTO atoms (
                atom_id, type, domain, statement, conditions, metrics,
                author_agent_id, signature,
                ph_attraction, ph_repulsion, ph_novelty, ph_disagreement,
                embedding_status, embedding
            ) VALUES ($1, 'hypothesis', $2, $3, '{}', NULL,
                    'test-agent', decode('deadbeef', 'hex'),
                    0.0, 0.0, 1.0, 0.0, 'ready', $4)
            "#
        )
        .bind(id)
        .bind(domain)
        .bind(statement)
        .bind(pg_vector)
        .execute(&pool)
        .await
        .expect("Failed to insert test atom");
    }

    // Query near atom1
    let vector = vec![1.0f32; 1];
    let mut query_vec = vec![0.0f32; 384];
    query_vec[0] = 1.0;
    let radius = 0.1; // tight radius — atom1 and atom2 should be in, atom3 out

    let result = queries::query_nearest_atom_with_density(&pool, query_vec, radius)
        .await
        .expect("Query should succeed");

    // Both atom1 and atom2 are in the same direction — just verify some atom is returned
    assert!(result.0.is_some(), "Should find nearest atom");
    let nearest = result.0.unwrap();
    assert!(nearest.atom_id == "atom1" || nearest.atom_id == "atom2");
}

#[serial]
#[tokio::test]
async fn test_get_domain_novelty_stats_empty() {
    let pool = setup_test_db().await;
    
    let stats = queries::get_domain_novelty_stats(&pool)
        .await
        .expect("Query should succeed");
    
    assert!(stats.is_empty()); // No domains in empty database
}

#[serial]
#[tokio::test]
async fn test_get_domain_novelty_stats_with_data() {
    let pool = setup_test_db().await;
    
    // Insert test atoms with different novelty values
    let domains = vec![
        ("domain1", "atom1", 0.8),
        ("domain1", "atom2", 0.6),
        ("domain1", "atom3", 0.4),
        ("domain2", "atom4", 0.9),
        ("domain2", "atom5", 0.7),
    ];
    
    for (domain, atom_id, novelty) in domains {
        sqlx::query(
            r#"
            INSERT INTO atoms (
                atom_id, type, domain, statement, conditions, metrics,
                author_agent_id, signature,
                ph_attraction, ph_repulsion, ph_novelty, ph_disagreement,
                embedding_status
            ) VALUES ($1, 'hypothesis', $2, 'Test statement', '{}', NULL,
                    'test-agent', decode('deadbeef', 'hex'),
                    0.0, 0.0, $3, 0.0, 'ready')
            "#
        )
        .bind(atom_id)
        .bind(domain)
        .bind(novelty as f64)
        .execute(&pool)
        .await
        .expect("Failed to insert test atom");
    }

    let stats = queries::get_domain_novelty_stats(&pool)
        .await
        .expect("Query should succeed");

    assert_eq!(stats.len(), 2); // Two domains

    // Check domain1: (0.8 + 0.6 + 0.4) / 3 = 0.6 (approx — REAL column has f32 precision)
    let domain1_stat = stats.iter().find(|(d, _)| d == "domain1").unwrap();
    assert!((domain1_stat.1 - 0.6).abs() < 1e-5, "domain1 avg = {}", domain1_stat.1);

    // Check domain2: (0.9 + 0.7) / 2 = 0.8
    let domain2_stat = stats.iter().find(|(d, _)| d == "domain2").unwrap();
    assert!((domain2_stat.1 - 0.8).abs() < 1e-5, "domain2 avg = {}", domain2_stat.1);
}

#[serial]
#[tokio::test]
async fn test_get_domain_novelty_stats_ignores_archived() {
    let pool = setup_test_db().await;
    
    // Insert test atoms - some archived
    let domains = vec![
        ("domain1", "atom1", 0.8, false),
        ("domain1", "atom2", 0.6, true),  // archived
        ("domain1", "atom3", 0.4, false),
    ];
    
    for (domain, atom_id, novelty, archived) in domains {
        sqlx::query(
            r#"
            INSERT INTO atoms (
                atom_id, type, domain, statement, conditions, metrics,
                author_agent_id, signature,
                ph_attraction, ph_repulsion, ph_novelty, ph_disagreement,
                embedding_status, archived
            ) VALUES ($1, 'hypothesis', $2, 'Test statement', '{}', NULL,
                    'test-agent', decode('deadbeef', 'hex'),
                    0.0, 0.0, $3, 0.0, 'ready', $4)
            "#
        )
        .bind(atom_id)
        .bind(domain)
        .bind(novelty as f64)
        .bind(archived)
        .execute(&pool)
        .await
        .expect("Failed to insert test atom");
    }
    
    let stats = queries::get_domain_novelty_stats(&pool)
        .await
        .expect("Query should succeed");
    
    assert_eq!(stats.len(), 1); // Only one domain

    // Should ignore archived atom: (0.8 + 0.4) / 2 = 0.6 (approx — REAL column)
    let domain1_stat = stats.iter().find(|(d, _)| d == "domain1").unwrap();
    assert!((domain1_stat.1 - 0.6).abs() < 1e-5, "domain1 avg = {}", domain1_stat.1);
}
