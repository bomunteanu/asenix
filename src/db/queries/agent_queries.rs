use crate::error::{MoteError, Result};
use crate::domain::agent::{Agent, AgentRegistration, AgentConfirmation};
use sqlx::{PgPool, Row};
use crate::crypto::{hashing::compute_agent_id, signing::generate_challenge};

pub async fn register_agent(pool: &PgPool, registration: AgentRegistration) -> Result<AgentRegistrationResponse> {
    let public_key = crate::crypto::signing::hex_to_bytes(&registration.public_key)?;
    let agent_id = compute_agent_id(&public_key);
    let challenge = generate_challenge();
    let challenge_hex = crate::crypto::signing::bytes_to_hex(&challenge);
    
    // Use runtime query instead of macro
    let row = sqlx::query(
        "INSERT INTO agents (agent_id, public_key, confirmed, challenge) VALUES ($1, $2, false, $3) RETURNING *"
    )
    .bind(&agent_id)
    .bind(&public_key)
    .bind(&challenge)
    .fetch_one(pool)
    .await?;
    
    Ok(AgentRegistrationResponse {
        agent_id: row.get("agent_id"),
        challenge: challenge_hex,
    })
}

pub async fn confirm_agent(pool: &PgPool, confirmation: AgentConfirmation) -> Result<()> {
    use crate::crypto::signing::{verify_signature, hex_to_bytes};
    
    // Get agent and challenge using runtime query
    let row = sqlx::query("SELECT * FROM agents WHERE agent_id = $1 AND confirmed = false")
        .bind(&confirmation.agent_id)
        .fetch_optional(pool)
        .await?
        .ok_or_else(|| MoteError::NotFound("Agent not found or already confirmed".to_string()))?;
    
    let agent = Agent {
        agent_id: row.get("agent_id"),
        public_key: row.get("public_key"),
        confirmed: row.get("confirmed"),
        challenge: row.get("challenge"),
        reliability: row.get("reliability"),
        replication_rate: row.get("replication_rate"),
        retraction_rate: row.get("retraction_rate"),
        contradiction_rate: row.get("contradiction_rate"),
        atoms_published: row.get("atoms_published"),
        created_at: row.get("created_at"),
    };
    
    let challenge = agent.challenge
        .ok_or_else(|| MoteError::Authentication("No challenge found for agent".to_string()))?;
    
    // Verify signature
    let signature = hex_to_bytes(&confirmation.signature)?;
    verify_signature(&agent.public_key, &challenge, &signature)?;
    
    // Confirm agent using runtime query
    sqlx::query("UPDATE agents SET confirmed = true, challenge = NULL WHERE agent_id = $1")
        .bind(&confirmation.agent_id)
        .execute(pool)
        .await?;
    
    Ok(())
}

pub async fn get_agent(pool: &PgPool, agent_id: &str) -> Result<Option<Agent>> {
    let row = sqlx::query("SELECT * FROM agents WHERE agent_id = $1")
        .bind(agent_id)
        .fetch_optional(pool)
        .await?;
    
    if let Some(row) = row {
        Ok(Some(Agent {
            agent_id: row.get("agent_id"),
            public_key: row.get("public_key"),
            confirmed: row.get("confirmed"),
            challenge: row.get("challenge"),
            reliability: row.get("reliability"),
            replication_rate: row.get("replication_rate"),
            retraction_rate: row.get("retraction_rate"),
            contradiction_rate: row.get("contradiction_rate"),
            atoms_published: row.get("atoms_published"),
            created_at: row.get("created_at"),
        }))
    } else {
        Ok(None)
    }
}

#[derive(Debug)]
pub struct AgentRegistrationResponse {
    pub agent_id: String,
    pub challenge: String,
}

#[derive(Debug)]
pub struct SimpleRegistrationResponse {
    pub agent_id: String,
    pub api_token: String,
}

/// Register an agent without requiring a client-side keypair.
/// Generates an Ed25519 keypair server-side (private key discarded),
/// derives agent_id from the public key, and issues a random API token
/// that can be used in place of per-request Ed25519 signatures.
pub async fn register_agent_simple(pool: &PgPool) -> Result<SimpleRegistrationResponse> {
    use rand::RngCore;

    let (_private_key, public_key) = crate::crypto::signing::generate_keypair();
    let agent_id = compute_agent_id(&public_key);

    let mut token_bytes = [0u8; 32];
    rand::rng().fill_bytes(&mut token_bytes);
    let api_token = format!("mote_{}", hex::encode(&token_bytes));

    sqlx::query(
        "INSERT INTO agents (agent_id, public_key, confirmed, api_token) VALUES ($1, $2, true, $3)",
    )
    .bind(&agent_id)
    .bind(&public_key)
    .bind(&api_token)
    .execute(pool)
    .await?;

    Ok(SimpleRegistrationResponse { agent_id, api_token })
}

/// Look up a confirmed agent by their API token.
pub async fn get_agent_by_token(pool: &PgPool, api_token: &str) -> Result<Option<Agent>> {
    let row = sqlx::query(
        "SELECT * FROM agents WHERE api_token = $1 AND confirmed = true",
    )
    .bind(api_token)
    .fetch_optional(pool)
    .await?;

    Ok(row.map(|row| Agent {
        agent_id: row.get("agent_id"),
        public_key: row.get("public_key"),
        confirmed: row.get("confirmed"),
        challenge: row.get("challenge"),
        reliability: row.get("reliability"),
        replication_rate: row.get("replication_rate"),
        retraction_rate: row.get("retraction_rate"),
        contradiction_rate: row.get("contradiction_rate"),
        atoms_published: row.get("atoms_published"),
        created_at: row.get("created_at"),
    }))
}
