use crate::error::Result;
use crate::domain::atom::Atom;
use sqlx::{PgPool, Row};
use super::atom_queries::atom_from_row;

pub struct ActiveClaim {
    pub claim_id: String,
    pub atom_id: String,
    pub agent_id: String,
    pub expires_at: chrono::DateTime<chrono::Utc>,
    pub hypothesis: String,
    pub domain: String,
    pub conditions: serde_json::Value,
}

/// Expire stale claims (lazy, called before any claim operation).
pub async fn expire_stale_claims(pool: &PgPool) -> Result<()> {
    sqlx::query("UPDATE claims SET active = false WHERE active = true AND expires_at < NOW()")
        .execute(pool)
        .await?;
    Ok(())
}

/// Insert a new claim row.
pub async fn create_claim(
    pool: &PgPool,
    claim_id: &str,
    atom_id: &str,
    agent_id: &str,
    expires_at: chrono::DateTime<chrono::Utc>,
) -> Result<()> {
    sqlx::query(
        "INSERT INTO claims (claim_id, atom_id, agent_id, expires_at, active) VALUES ($1, $2, $3, $4, true)",
    )
    .bind(claim_id)
    .bind(atom_id)
    .bind(agent_id)
    .bind(expires_at)
    .execute(pool)
    .await?;
    Ok(())
}

/// Return all active claims in a given domain (joined with the claimed atom for statement/conditions).
pub async fn get_active_claims_in_domain(
    pool: &PgPool,
    domain: &str,
) -> Result<Vec<ActiveClaim>> {
    let rows = sqlx::query(
        "SELECT c.claim_id, c.atom_id, c.agent_id, c.expires_at,
                a.statement AS hypothesis, a.domain, a.conditions
         FROM claims c
         JOIN atoms a ON a.atom_id = c.atom_id
         WHERE c.active = true AND a.domain = $1
         ORDER BY c.expires_at DESC",
    )
    .bind(domain)
    .fetch_all(pool)
    .await?;

    Ok(rows
        .into_iter()
        .map(|r| ActiveClaim {
            claim_id: r.get("claim_id"),
            atom_id: r.get("atom_id"),
            agent_id: r.get("agent_id"),
            expires_at: r.get("expires_at"),
            hypothesis: r.get("hypothesis"),
            domain: r.get("domain"),
            conditions: r.get("conditions"),
        })
        .collect())
}

/// Return atoms in the same domain as the neighbourhood context (non-retracted, non-archived).
/// Used to populate the `neighbourhood` field in claim_direction responses.
pub async fn get_neighbourhood_atoms(
    pool: &PgPool,
    domain: &str,
    limit: i64,
) -> Result<Vec<Atom>> {
    let rows = sqlx::query(
        "SELECT * FROM atoms
         WHERE domain = $1 AND NOT retracted AND NOT archived
         ORDER BY ph_attraction DESC, created_at DESC
         LIMIT $2",
    )
    .bind(domain)
    .bind(limit)
    .fetch_all(pool)
    .await?;

    rows.into_iter().map(atom_from_row).collect()
}
