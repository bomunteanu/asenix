use crate::error::{MoteError, Result};
use crate::domain::agent::{Agent, AgentRegistration, AgentConfirmation};
use crate::domain::atom::{Atom, AtomInput, AtomType};
use sqlx::{PgPool, Row};
use crate::crypto::{hashing::{compute_atom_id, compute_agent_id}, signing::generate_challenge};

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

// Phase 3: Core Graph Operations

pub async fn publish_atom(
    pool: &PgPool,
    agent_id: &str,
    atom_input: AtomInput,
) -> Result<String> {
    let timestamp = chrono::Utc::now();
    let atom_id = compute_atom_id(
        &atom_input.atom_type.to_string(),
        &atom_input.domain,
        &atom_input.statement,
        &atom_input.conditions,
        &atom_input.provenance,
        &timestamp,
    );
    
    let mut tx = pool.begin().await.map_err(MoteError::Database)?;
    
    sqlx::query(
        "INSERT INTO atoms (atom_id, type, domain, statement, conditions, metrics, provenance, signature, author_agent_id, created_at, embedding_status, lifecycle, retracted)
         VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, NOW(), 'pending', 'provisional', false)"
    )
    .bind(&atom_id)
    .bind(atom_input.atom_type.to_string())
    .bind(&atom_input.domain)
    .bind(&atom_input.statement)
    .bind(&atom_input.conditions)
    .bind(&atom_input.metrics)
    .bind(&atom_input.provenance)
    .bind(&atom_input.signature)
    .bind(agent_id)
    .execute(&mut *tx)
    .await
    .map_err(MoteError::Database)?;
    
    tx.commit().await.map_err(MoteError::Database)?;
    
    // TODO: Update graph cache incrementally
    // This would require access to the graph cache, which is typically handled at the handler level
    
    Ok(atom_id)
}

pub async fn search_atoms(
    pool: &PgPool,
    domain_filter: Option<&str>,
    type_filter: Option<&str>,
    lifecycle_filter: Option<&str>,
    limit: i64,
    offset: i64,
) -> Result<Vec<Atom>> {
    let mut query = "SELECT * FROM atoms WHERE NOT retracted AND NOT archived".to_string();
    let mut bind_count = 0;

    if let Some(_domain) = domain_filter {
        bind_count += 1;
        query.push_str(&format!(" AND domain = ${}", bind_count));
    }

    if let Some(_atom_type) = type_filter {
        bind_count += 1;
        query.push_str(&format!(" AND type = ${}", bind_count));
    }

    if let Some(_lifecycle) = lifecycle_filter {
        bind_count += 1;
        query.push_str(&format!(" AND lifecycle = ${}", bind_count));
    }

    query.push_str(" ORDER BY created_at DESC");
    query.push_str(&format!(" LIMIT {} OFFSET {}", limit, offset));

    let mut query_builder = sqlx::query(&query);

    if let Some(domain) = domain_filter {
        query_builder = query_builder.bind(domain);
    }
    if let Some(atom_type) = type_filter {
        query_builder = query_builder.bind(atom_type);
    }
    if let Some(lifecycle) = lifecycle_filter {
        query_builder = query_builder.bind(lifecycle);
    }

    let rows = query_builder.fetch_all(pool).await?;
    
    let mut atoms = Vec::new();
    for row in rows {
        let atom_type_str: String = row.get("type");
        let atom_type = match atom_type_str.as_str() {
            "hypothesis" => AtomType::Hypothesis,
            "finding" => AtomType::Finding,
            "negative_result" => AtomType::NegativeResult,
            "delta" => AtomType::Delta,
            "experiment_log" => AtomType::ExperimentLog,
            "synthesis" => AtomType::Synthesis,
            "bounty" => AtomType::Bounty,
            _ => return Err(MoteError::Validation(format!("Unknown atom type: {}", atom_type_str))),
        };

        let lifecycle_str: String = row.get("lifecycle");
        let lifecycle = match lifecycle_str.as_str() {
            "provisional" => crate::domain::atom::Lifecycle::Provisional,
            "replicated" => crate::domain::atom::Lifecycle::Replicated,
            "core" => crate::domain::atom::Lifecycle::Core,
            "contested" => crate::domain::atom::Lifecycle::Contested,
            _ => return Err(MoteError::Validation(format!("Unknown lifecycle: {}", lifecycle_str))),
        };

        let embedding_status_str: String = row.get("embedding_status");
        let embedding_status = match embedding_status_str.as_str() {
            "pending" => crate::domain::atom::EmbeddingStatus::Pending,
            "ready" => crate::domain::atom::EmbeddingStatus::Ready,
            _ => return Err(MoteError::Validation(format!("Unknown embedding status: {}", embedding_status_str))),
        };

        atoms.push(Atom {
            atom_id: row.get("atom_id"),
            atom_type,
            domain: row.get("domain"),
            statement: row.get("statement"),
            conditions: row.get("conditions"),
            metrics: row.get("metrics"),
            provenance: row.get("provenance"),
            author_agent_id: row.get("author_agent_id"),
            created_at: row.get("created_at"),
            signature: row.get("signature"),
            confidence: row.get::<f32, _>("confidence") as f64,
            ph_attraction: row.get::<f32, _>("ph_attraction") as f64,
            ph_repulsion: row.get::<f32, _>("ph_repulsion") as f64,
            ph_novelty: row.get::<f32, _>("ph_novelty") as f64,
            ph_disagreement: row.get::<f32, _>("ph_disagreement") as f64,
            embedding: None, // Skip embedding decoding for now - search doesn't need it
            embedding_status,
            repl_exact: row.get("repl_exact"),
            repl_conceptual: row.get("repl_conceptual"),
            repl_extension: row.get("repl_extension"),
            traffic: row.get("traffic"),
            lifecycle,
            retracted: row.get("retracted"),
            retraction_reason: row.get("retraction_reason"),
            ban_flag: row.get("ban_flag"),
            archived: row.get("archived"),
            probationary: row.get("probationary"),
            summary: row.get("summary"),
        });
    }

    Ok(atoms)
}

pub async fn get_synthesis_atoms(
    pool: &PgPool,
    domain_filter: Option<&str>,
) -> Result<Vec<Atom>> {
    let mut query = "SELECT * FROM atoms WHERE type = 'synthesis' AND NOT retracted AND NOT archived".to_string();
    let mut bind_count = 0;

    if let Some(_domain) = domain_filter {
        bind_count += 1;
        query.push_str(&format!(" AND domain = ${}", bind_count));
    }

    query.push_str(" ORDER BY created_at DESC");

    let mut query_builder = sqlx::query(&query);

    if let Some(domain) = domain_filter {
        query_builder = query_builder.bind(domain);
    }

    let rows = query_builder.fetch_all(pool).await?;
    
    let mut atoms = Vec::new();
    for row in rows {
        let atom_type_str: String = row.get("type");
        let atom_type = match atom_type_str.as_str() {
            "synthesis" => AtomType::Synthesis,
            _ => return Err(MoteError::Validation(format!("Unexpected atom type: {}", atom_type_str))),
        };

        let lifecycle_str: String = row.get("lifecycle");
        let lifecycle = match lifecycle_str.as_str() {
            "provisional" => crate::domain::atom::Lifecycle::Provisional,
            "replicated" => crate::domain::atom::Lifecycle::Replicated,
            "core" => crate::domain::atom::Lifecycle::Core,
            "contested" => crate::domain::atom::Lifecycle::Contested,
            _ => return Err(MoteError::Validation(format!("Unknown lifecycle: {}", lifecycle_str))),
        };

        let embedding_status_str: String = row.get("embedding_status");
        let embedding_status = match embedding_status_str.as_str() {
            "pending" => crate::domain::atom::EmbeddingStatus::Pending,
            "ready" => crate::domain::atom::EmbeddingStatus::Ready,
            _ => return Err(MoteError::Validation(format!("Unknown embedding status: {}", embedding_status_str))),
        };

        atoms.push(Atom {
            atom_id: row.get("atom_id"),
            atom_type,
            domain: row.get("domain"),
            statement: row.get("statement"),
            conditions: row.get("conditions"),
            metrics: row.get("metrics"),
            provenance: row.get("provenance"),
            author_agent_id: row.get("author_agent_id"),
            created_at: row.get("created_at"),
            signature: row.get("signature"),
            confidence: row.get::<f32, _>("confidence") as f64,
            ph_attraction: row.get::<f32, _>("ph_attraction") as f64,
            ph_repulsion: row.get::<f32, _>("ph_repulsion") as f64,
            ph_novelty: row.get::<f32, _>("ph_novelty") as f64,
            ph_disagreement: row.get::<f32, _>("ph_disagreement") as f64,
            embedding: None, // Skip embedding decoding for now - search doesn't need it
            embedding_status,
            repl_exact: row.get("repl_exact"),
            repl_conceptual: row.get("repl_conceptual"),
            repl_extension: row.get("repl_extension"),
            traffic: row.get("traffic"),
            lifecycle,
            retracted: row.get("retracted"),
            retraction_reason: row.get("retraction_reason"),
            ban_flag: row.get("ban_flag"),
            archived: row.get("archived"),
            probationary: row.get("probationary"),
            summary: row.get("summary"),
        });
    }

    Ok(atoms)
}

pub async fn retract_atom(
    pool: &PgPool,
    atom_id: &str,
    agent_id: &str,
    reason: Option<&str>,
) -> Result<()> {
    // Verify author
    let author: String = sqlx::query_scalar("SELECT author_agent_id FROM atoms WHERE atom_id = $1")
        .bind(atom_id)
        .fetch_one(pool)
        .await
        .map_err(|_| MoteError::NotFound("Atom not found".to_string()))?;

    if author != agent_id {
        return Err(MoteError::Authentication("Only author can retract atom".to_string()));
    }

    // Mark as retracted
    sqlx::query("UPDATE atoms SET retracted = true, retraction_reason = $1 WHERE atom_id = $2")
        .bind(reason)
        .bind(atom_id)
        .execute(pool)
        .await?;

    Ok(())
}
