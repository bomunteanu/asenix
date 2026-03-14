use crate::error::Result;
use crate::domain::atom::Atom;
use sqlx::{PgPool, Row};
use super::atom_queries::atom_from_row;

pub struct ClusterResult {
    pub atom: Atom,
    pub distance: f64,
}

/// Find atoms within `radius` cosine distance of `vector`.
/// Only atoms with computed embeddings are considered.
pub async fn query_cluster_atoms(
    pool: &PgPool,
    vector: Vec<f32>,
    radius: f64,
    limit: i64,
) -> Result<Vec<ClusterResult>> {
    let pg_vector = pgvector::Vector::from(vector);

    let rows = sqlx::query(
        "SELECT *, (embedding <=> $1) AS distance
         FROM atoms
         WHERE embedding IS NOT NULL
               AND NOT retracted AND NOT archived
               AND (embedding <=> $1) < $2
         ORDER BY distance
         LIMIT $3",
    )
    .bind(pg_vector)
    .bind(radius as f32)
    .bind(limit)
    .fetch_all(pool)
    .await?;

    rows.into_iter()
        .map(|row| {
            let distance: f32 = row.get("distance");
            let atom = atom_from_row(row)?;
            Ok(ClusterResult { atom, distance: distance as f64 })
        })
        .collect()
}

/// Find the nearest atom to a vector and count atoms within radius
/// Used for exploration mode in get_suggestions
pub async fn query_nearest_atom_with_density(
    pool: &PgPool,
    vector: Vec<f32>,
    radius: f32,
) -> Result<(Option<Atom>, i64)> {
    let pg_vector = pgvector::Vector::from(vector);

    // Find nearest atom and count atoms within radius in one query
    let row = sqlx::query(
        r#"
        WITH nearest_atom AS (
            SELECT *, (embedding <=> $1) AS distance
            FROM atoms
            WHERE embedding IS NOT NULL
                  AND NOT retracted AND NOT archived
            ORDER BY distance
            LIMIT 1
        ),
        atom_count AS (
            SELECT COUNT(*) as count
            FROM atoms
            WHERE embedding IS NOT NULL
                  AND NOT retracted AND NOT archived
                  AND (embedding <=> $1) < $2
        )
        SELECT 
            na.atom_id, na.type, na.domain, na.statement, na.conditions, 
            na.metrics, na.provenance, na.author_agent_id, na.created_at, 
            na.signature, na.artifact_tree_hash, na.confidence, 
            na.ph_attraction, na.ph_repulsion, na.ph_novelty, na.ph_disagreement,
            na.embedding_status, na.repl_exact, na.repl_conceptual, 
            na.repl_extension, na.traffic, na.lifecycle, na.retracted, 
            na.retraction_reason, na.ban_flag, na.archived, na.probationary, 
            na.summary, na.distance,
            ac.count as atom_count
        FROM nearest_atom na, atom_count ac
        "#
    )
    .bind(pg_vector)
    .bind(radius)
    .fetch_optional(pool)
    .await?;

    match row {
        Some(row) => {
            let atom_count: i64 = row.get("atom_count");
            let atom = atom_from_row(row)?;
            
            Ok((Some(atom), atom_count))
        }
        None => Ok((None, 0))
    }
}

/// Get mean novelty per domain for bounty detection
/// Returns domains with their mean ph_novelty across top 50 atoms
pub async fn get_domain_novelty_stats(
    pool: &PgPool,
) -> Result<Vec<(String, f64)>> {
    let rows = sqlx::query(
        r#"
        WITH top_atoms_per_domain AS (
            SELECT 
                domain,
                ph_novelty,
                ROW_NUMBER() OVER (PARTITION BY domain ORDER BY ph_novelty DESC) as rn
            FROM atoms 
            WHERE NOT archived 
            AND NOT retracted
            AND ph_novelty IS NOT NULL
        )
        SELECT 
            domain,
            AVG(ph_novelty) as mean_novelty
        FROM top_atoms_per_domain 
        WHERE rn <= 50
        GROUP BY domain
        HAVING COUNT(*) > 0
        "#
    )
    .fetch_all(pool)
    .await?;

    let results = rows.into_iter()
        .map(|row| {
            let domain: String = row.get("domain");
            let mean_novelty: f64 = row.get("mean_novelty");
            (domain, mean_novelty)
        })
        .collect();

    Ok(results)
}
