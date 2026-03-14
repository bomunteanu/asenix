//! Unit tests for the bounty worker.
//!
//! These tests exercise BountyWorker against a real (test) database to verify the
//! three key behavioural invariants:
//!
//!  1. A bounty atom IS published when mean novelty exceeds the threshold and a sparse
//!     region (< sparse_region_max_atoms nearby atoms) exists.
//!  2. A bounty atom is NOT published when every sampled region has >= sparse_region_max_atoms
//!     nearby atoms (dense domain).
//!  3. A domain with zero atoms is simply skipped — no bounty, no error.

use asenix::workers::bounty::BountyWorker;
use pgvector::Vector;
use serial_test::serial;
use sqlx::PgPool;
use std::env;

// ---------------------------------------------------------------------------
// Test helpers
// ---------------------------------------------------------------------------

async fn setup_test_db() -> PgPool {
    let database_url = env::var("DATABASE_URL").unwrap_or_else(|_| {
        "postgres://mote:mote_password@localhost:5432/mote_test".to_string()
    });

    let pool = sqlx::PgPool::connect(&database_url)
        .await
        .expect("Failed to connect to test database");

    sqlx::query("TRUNCATE TABLE atoms CASCADE")
        .execute(&pool)
        .await
        .expect("Failed to truncate atoms");
    sqlx::query("TRUNCATE TABLE agents CASCADE")
        .execute(&pool)
        .await
        .expect("Failed to truncate agents");

    // Insert a test author agent.
    sqlx::query(
        "INSERT INTO agents (agent_id, public_key, confirmed, created_at) \
         VALUES ('test-agent', decode('deadbeef', 'hex'), true, NOW()) \
         ON CONFLICT (agent_id) DO NOTHING",
    )
    .execute(&pool)
    .await
    .expect("Failed to insert test agent");

    pool
}

/// Insert an atom with a known 384-dim embedding and explicit novelty value.
async fn insert_atom_with_embedding(
    pool: &PgPool,
    atom_id: &str,
    domain: &str,
    novelty: f32,
    embedding: Vec<f32>,
) {
    let pg_vector = Vector::from(embedding);
    sqlx::query(
        r#"
        INSERT INTO atoms (
            atom_id, type, domain, statement, conditions, metrics,
            author_agent_id, signature,
            ph_attraction, ph_repulsion, ph_novelty, ph_disagreement,
            embedding_status, embedding
        ) VALUES ($1, 'hypothesis', $2, 'Test statement', '{}', NULL,
                  'test-agent', decode('deadbeef', 'hex'),
                  0.0, 0.0, $3, 0.0, 'ready', $4)
        "#,
    )
    .bind(atom_id)
    .bind(domain)
    .bind(novelty as f64)
    .bind(pg_vector)
    .execute(pool)
    .await
    .expect("Failed to insert test atom with embedding");
}

/// Count bounty atoms in a given domain.
async fn count_bounty_atoms(pool: &PgPool, domain: &str) -> i64 {
    sqlx::query_scalar::<_, i64>(
        "SELECT COUNT(*) FROM atoms WHERE type = 'bounty' AND domain = $1",
    )
    .bind(domain)
    .fetch_one(pool)
    .await
    .expect("Failed to count bounty atoms")
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

/// Bounty is published when mean novelty exceeds the threshold AND a sparse
/// region (atom_count < sparse_max) is found.
///
/// Setup: domain "sparse-domain" has 3 atoms with novelty=0.9.  We use a very
/// large sparse_region_max_atoms (1 000) so any sample is treated as sparse,
/// and a tight density radius so the sampled neighbourhoods are always empty
/// (atom_count = 0 < 1000).  The worker publishes exactly one bounty atom.
#[serial]
#[tokio::test]
async fn test_bounty_published_when_novelty_exceeds_threshold() {
    let pool = setup_test_db().await;

    // Three atoms in different directions so at least one random sample lands
    // near one of them.
    let mut emb1 = vec![0.0f32; 384];
    emb1[0] = 1.0;
    let mut emb2 = vec![0.0f32; 384];
    emb2[1] = 1.0;
    let mut emb3 = vec![0.0f32; 384];
    emb3[2] = 1.0;

    insert_atom_with_embedding(&pool, "a1", "sparse-domain", 0.9, emb1).await;
    insert_atom_with_embedding(&pool, "a2", "sparse-domain", 0.9, emb2).await;
    insert_atom_with_embedding(&pool, "a3", "sparse-domain", 0.9, emb3).await;

    // Threshold = 0.7, mean novelty = 0.9 → domain qualifies.
    // sparse_max = 1000 → any region is considered sparse.
    // density_radius = 0.001 → tiny neighbourhood, atom_count will be 0 or 1.
    // exploration_samples = 20 → multiple chances to find a nearest atom.
    let worker = BountyWorker::new(
        pool.clone(),
        0.7,    // novelty_threshold
        20,     // exploration_samples
        0.001,  // exploration_density_radius (tiny — low density guaranteed)
        384,    // embedding_dimension
        1000,   // sparse_region_max_atoms
    );

    let count = worker
        .run_bounty_tick()
        .await
        .expect("run_bounty_tick failed");

    assert!(count > 0, "Expected at least one bounty to be published, got {}", count);
    assert!(
        count_bounty_atoms(&pool, "sparse-domain").await > 0,
        "Expected a bounty atom in the DB for sparse-domain"
    );
}

/// Bounty is NOT published when every sampled region is dense
/// (atom_count >= sparse_region_max_atoms).
///
/// Setup: 8 atoms placed in 8 orthogonal directions across the first 8 basis
/// vectors, all with novelty=0.9.  We use density_radius=1.9 (almost the full
/// cosine-distance range of [0, 2]) so any random unit vector lands within 1.9
/// of nearly all of them — making atom_count well above sparse_max=3.
#[serial]
#[tokio::test]
async fn test_bounty_skipped_when_region_is_dense() {
    let pool = setup_test_db().await;

    // 8 basis-vector atoms covering 8 orthogonal directions.
    for i in 0..8usize {
        let mut emb = vec![0.0f32; 384];
        emb[i] = 1.0;
        insert_atom_with_embedding(
            &pool,
            &format!("dense-a{}", i),
            "dense-domain",
            0.9,
            emb,
        )
        .await;
    }

    // With radius 1.9 virtually every random direction has at least 6 atoms
    // within range (cosine distance from a basis vector to an orthogonal one
    // is 1.0 < 1.9).  sparse_max=3 → all regions are dense → no bounty.
    let worker = BountyWorker::new(
        pool.clone(),
        0.7,   // novelty_threshold (mean novelty 0.9 qualifies)
        30,    // exploration_samples (many tries, all should be dense)
        1.9,   // exploration_density_radius (large — finds many neighbours)
        384,   // embedding_dimension
        3,     // sparse_region_max_atoms (strict — only <3 counts as sparse)
    );

    let count = worker
        .run_bounty_tick()
        .await
        .expect("run_bounty_tick failed");

    assert_eq!(count, 0, "Expected no bounties for dense domain, got {}", count);
    assert_eq!(
        count_bounty_atoms(&pool, "dense-domain").await,
        0,
        "Expected zero bounty atoms in DB for dense-domain"
    );
}

/// Zero-atom domains (empty DB) are silently ignored — Ok(0) returned.
#[serial]
#[tokio::test]
async fn test_zero_atom_domain_ignored() {
    let pool = setup_test_db().await;
    // DB is empty after setup — no atoms, no domains.

    let worker = BountyWorker::new(
        pool.clone(),
        0.7,  // novelty_threshold
        5,    // exploration_samples
        0.5,  // exploration_density_radius
        384,  // embedding_dimension
        3,    // sparse_region_max_atoms
    );

    let count = worker
        .run_bounty_tick()
        .await
        .expect("run_bounty_tick should succeed on empty DB");

    assert_eq!(count, 0, "Expected 0 bounties for empty DB");
}
