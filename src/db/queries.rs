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
    
    // Validate artifact_tree_hash if provided
    if let Some(ref artifact_hash) = atom_input.artifact_tree_hash {
        // Check if artifact exists and is a tree
        let artifact_check = sqlx::query(
            "SELECT type FROM artifacts WHERE hash = $1"
        )
        .bind(artifact_hash)
        .fetch_optional(pool)
        .await
        .map_err(MoteError::Database)?;

        match artifact_check {
            Some(row) => {
                let artifact_type: String = row.get("type");
                if artifact_type != "tree" {
                    return Err(MoteError::Validation(
                        format!("Artifact {} exists but is not a tree", artifact_hash)
                    ));
                }
            }
            None => {
                return Err(MoteError::Validation(
                    format!("Artifact {} does not exist. Upload the artifact tree first.", artifact_hash)
                ));
            }
        }
    }

    let mut tx = pool.begin().await.map_err(MoteError::Database)?;
    
    sqlx::query(
        "INSERT INTO atoms (atom_id, type, domain, statement, conditions, metrics, provenance, signature, author_agent_id, created_at, embedding_status, lifecycle, retracted, artifact_tree_hash)
         VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, NOW(), 'pending', 'provisional', false, $10)"
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
    .bind(&atom_input.artifact_tree_hash)
    .execute(&mut *tx)
    .await
    .map_err(MoteError::Database)?;
    
    tx.commit().await.map_err(MoteError::Database)?;
    
    // TODO: Update graph cache incrementally
    // This would require access to the graph cache, which is typically handled at the handler level
    
    Ok(atom_id)
}

pub async fn get_atom(pool: &PgPool, atom_id: &str) -> Result<Atom> {
    let row = sqlx::query("SELECT * FROM atoms WHERE atom_id = $1 AND NOT retracted AND NOT archived")
        .bind(atom_id)
        .fetch_one(pool)
        .await
        .map_err(MoteError::Database)?;
    
    let atom: Atom = serde_json::from_value(row.get("atom_data"))
        .map_err(|e| MoteError::Validation(format!("Failed to deserialize atom: {}", e)))?;
    
    Ok(atom)
}

pub async fn search_atoms(
    pool: &PgPool,
    domain_filter: Option<&str>,
    type_filter: Option<&str>,
    lifecycle_filter: Option<&str>,
    text_search: Option<&str>,
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

    if let Some(_text) = text_search {
        bind_count += 1;
        query.push_str(&format!(" AND statement ILIKE ${}", bind_count));
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
    if let Some(text) = text_search {
        query_builder = query_builder.bind(format!("%{}%", text));
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
            artifact_tree_hash: row.get("artifact_tree_hash"),
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
            artifact_tree_hash: row.get("artifact_tree_hash"),
            confidence: row.get::<f32, _>("confidence") as f64,
            ph_attraction: row.get::<f32, _>("ph_attraction") as f64,
            ph_repulsion: row.get::<f32, _>("ph_repulsion") as f64,
            ph_novelty: row.get::<f32, _>("ph_novelty") as f64,
            ph_disagreement: row.get::<f32, _>("ph_disagreement") as f64,
            embedding: None, // Skip embedding decoding for now - synthesis doesn't need it
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

/// Vector search

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

/// Pheromone and contradiction support

/// A contradiction found between two atoms.
pub struct Contradiction {
    pub existing_atom_id: String,
    pub existing_statement: String,
    pub conflicting_metrics: Vec<String>, // metric names that oppose
}

/// Find atoms in the same domain whose conditions overlap with `conditions`
/// and whose metrics conflict with `metrics` on shared names.
/// Two metrics conflict when they have the same name and opposing `direction` values.
pub async fn find_contradicting_atoms(
    pool: &PgPool,
    domain: &str,
    exclude_atom_id: &str,
    conditions: &serde_json::Value,
    metrics: &Option<serde_json::Value>,
) -> Result<Vec<Contradiction>> {
    let Some(metrics_arr) = metrics.as_ref().and_then(|m| m.as_array()) else {
        return Ok(vec![]); // no metrics → no auto-contradiction
    };
    if metrics_arr.is_empty() {
        return Ok(vec![]);
    }

    // Collect (name, direction) pairs for the new atom
    let new_metric_dirs: std::collections::HashMap<String, String> = metrics_arr.iter()
        .filter_map(|m| {
            let name = m.get("name")?.as_str()?.to_string();
            let dir  = m.get("direction")?.as_str()?.to_string();
            Some((name, dir))
        })
        .collect();

    if new_metric_dirs.is_empty() {
        return Ok(vec![]);
    }

    // Fetch candidates: same domain, has metrics, not retracted/archived
    let rows = sqlx::query(
        "SELECT atom_id, statement, conditions, metrics
         FROM atoms
         WHERE domain = $1 AND atom_id != $2
               AND NOT retracted AND NOT archived
               AND metrics IS NOT NULL",
    )
    .bind(domain)
    .bind(exclude_atom_id)
    .fetch_all(pool)
    .await?;

    let mut contradictions = Vec::new();

    for row in rows {
        let existing_conditions: serde_json::Value = row.get("conditions");
        // Conditions are equivalent when ALL shared keys have matching values.
        // (Manifesto: "equivalent when all shared keys match")
        // If there are no shared keys, the atoms are not comparable → no contradiction.
        let conditions_equivalent = match (conditions.as_object(), existing_conditions.as_object()) {
            (Some(new_conds), Some(ex_conds)) => {
                let shared_keys: Vec<&String> = new_conds.keys()
                    .filter(|k| ex_conds.contains_key(*k))
                    .collect();
                !shared_keys.is_empty() &&
                    shared_keys.iter().all(|k| new_conds.get(*k) == ex_conds.get(*k))
            }
            _ => false,
        };

        if !conditions_equivalent {
            continue;
        }

        let existing_metrics: serde_json::Value = row.get("metrics");
        let Some(ex_arr) = existing_metrics.as_array() else { continue };

        // Find metric names where directions oppose
        let opposing_direction = |d: &str| -> &str {
            if d == "higher_better" { "lower_better" } else { "higher_better" }
        };

        let conflicting: Vec<String> = ex_arr.iter()
            .filter_map(|m| {
                let name = m.get("name")?.as_str()?.to_string();
                let dir  = m.get("direction")?.as_str()?;
                if let Some(new_dir) = new_metric_dirs.get(&name) {
                    if new_dir.as_str() == opposing_direction(dir) {
                        return Some(name);
                    }
                    // Same direction but dramatically different value → also flag
                    // (skip for now; pure direction-flip is sufficient for v0)
                }
                None
            })
            .collect();

        if !conflicting.is_empty() {
            contradictions.push(Contradiction {
                existing_atom_id: row.get("atom_id"),
                existing_statement: row.get("statement"),
                conflicting_metrics: conflicting,
            });
        }
    }

    Ok(contradictions)
}

/// Bump ph_attraction on all atoms in the same domain when a positive finding is published.
/// `metric_improvement` is the magnitude of improvement (0.0–1.0 scale, capped).
pub async fn update_pheromone_attraction(
    pool: &PgPool,
    domain: &str,
    exclude_atom_id: &str,
    delta: f64,
) -> Result<()> {
    let capped = delta.min(1.0_f64).max(0.0_f64);
    sqlx::query(
        "UPDATE atoms SET ph_attraction = LEAST(ph_attraction + $1, 10.0)
         WHERE domain = $2 AND atom_id != $3 AND NOT retracted AND NOT archived",
    )
    .bind(capped as f32)
    .bind(domain)
    .bind(exclude_atom_id)
    .execute(pool)
    .await?;
    Ok(())
}

/// Bump ph_disagreement on two atoms that contradict each other.
pub async fn update_pheromone_disagreement(
    pool: &PgPool,
    atom_id_a: &str,
    atom_id_b: &str,
) -> Result<()> {
    sqlx::query(
        "UPDATE atoms SET ph_disagreement = LEAST(ph_disagreement + 0.1, 1.0)
         WHERE atom_id = ANY($1)",
    )
    .bind(&[atom_id_a, atom_id_b][..])
    .execute(pool)
    .await?;
    Ok(())
}

/// Claim management

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

fn atom_from_row(row: sqlx::postgres::PgRow) -> Result<Atom> {
    use sqlx::Row as _;
    let atom_type = match row.get::<String, _>("type").as_str() {
        "hypothesis"      => AtomType::Hypothesis,
        "finding"         => AtomType::Finding,
        "negative_result" => AtomType::NegativeResult,
        "delta"           => AtomType::Delta,
        "experiment_log"  => AtomType::ExperimentLog,
        "synthesis"       => AtomType::Synthesis,
        "bounty"          => AtomType::Bounty,
        t => return Err(MoteError::Validation(format!("Unknown atom type: {}", t))),
    };
    let lifecycle = match row.get::<String, _>("lifecycle").as_str() {
        "provisional" => crate::domain::atom::Lifecycle::Provisional,
        "replicated"  => crate::domain::atom::Lifecycle::Replicated,
        "core"        => crate::domain::atom::Lifecycle::Core,
        "contested"   => crate::domain::atom::Lifecycle::Contested,
        l => return Err(MoteError::Validation(format!("Unknown lifecycle: {}", l))),
    };
    let embedding_status = match row.get::<String, _>("embedding_status").as_str() {
        "pending" => crate::domain::atom::EmbeddingStatus::Pending,
        "ready"   => crate::domain::atom::EmbeddingStatus::Ready,
        s => return Err(MoteError::Validation(format!("Unknown embedding status: {}", s))),
    };
    Ok(Atom {
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
        artifact_tree_hash: row.get("artifact_tree_hash"),
        confidence: row.get::<f32, _>("confidence") as f64,
        ph_attraction: row.get::<f32, _>("ph_attraction") as f64,
        ph_repulsion: row.get::<f32, _>("ph_repulsion") as f64,
        ph_novelty: row.get::<f32, _>("ph_novelty") as f64,
        ph_disagreement: row.get::<f32, _>("ph_disagreement") as f64,
        embedding: None,
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
    })
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

// Review queue functions

#[derive(Debug)]
pub struct ReviewQueueItem {
    pub atom_id: String,
    pub atom_type: String,
    pub domain: String,
    pub statement: String,
    pub author_agent_id: String,
    pub created_at: chrono::DateTime<chrono::Utc>,
    pub review_status: String,
    pub auto_review_eligible: bool,
}

/// Get atoms pending review, with optional filtering
pub async fn get_review_queue(
    pool: &PgPool,
    limit: i64,
    offset: i64,
    domain_filter: Option<&str>,
) -> Result<Vec<ReviewQueueItem>> {
    let query = if let Some(domain) = domain_filter {
        sqlx::query(
            "SELECT a.atom_id, a.type, a.domain, a.statement, a.author_agent_id, a.created_at, a.review_status,
                    CASE WHEN ag.reliability >= 0.8 AND ag.atoms_published >= 5 THEN true ELSE false END as auto_review_eligible
             FROM atoms a
             JOIN agents ag ON a.author_agent_id = ag.agent_id
             WHERE a.review_status = 'pending' AND a.domain = $1
             ORDER BY a.created_at DESC
             LIMIT $2 OFFSET $3"
        )
        .bind(domain)
        .bind(limit)
        .bind(offset)
    } else {
        sqlx::query(
            "SELECT a.atom_id, a.type, a.domain, a.statement, a.author_agent_id, a.created_at, a.review_status,
                    CASE WHEN ag.reliability >= 0.8 AND ag.atoms_published >= 5 THEN true ELSE false END as auto_review_eligible
             FROM atoms a
             JOIN agents ag ON a.author_agent_id = ag.agent_id
             WHERE a.review_status = 'pending'
             ORDER BY a.created_at DESC
             LIMIT $1 OFFSET $2"
        )
        .bind(limit)
        .bind(offset)
    };

    let rows = query
        .fetch_all(pool)
        .await?;

    rows.into_iter()
        .map(|row| {
            Ok(ReviewQueueItem {
                atom_id: row.get("atom_id"),
                atom_type: row.get("type"),
                domain: row.get("domain"),
                statement: row.get("statement"),
                author_agent_id: row.get("author_agent_id"),
                created_at: row.get("created_at"),
                review_status: row.get("review_status"),
                auto_review_eligible: row.get("auto_review_eligible"),
            })
        })
        .collect()
}

/// Get total count of pending review items
pub async fn get_review_queue_count(
    pool: &PgPool,
    domain_filter: Option<&str>,
) -> Result<i64> {
    let count: i64 = if let Some(domain) = domain_filter {
        sqlx::query_scalar(
            "SELECT COUNT(*) 
             FROM atoms a
             WHERE a.review_status = 'pending' AND a.domain = $1"
        )
        .bind(domain)
        .fetch_one(pool)
        .await?
    } else {
        sqlx::query_scalar(
            "SELECT COUNT(*) 
             FROM atoms a
             WHERE a.review_status = 'pending'"
        )
        .fetch_one(pool)
        .await?
    };
    
    Ok(count)
}

#[derive(Debug)]
pub struct ReviewRecord {
    pub review_id: String,
    pub atom_id: String,
    pub reviewer_agent_id: String,
    pub decision: String,
    pub reason: Option<String>,
    pub created_at: chrono::DateTime<chrono::Utc>,
}

/// Create a review record and update atom status
pub async fn create_review(
    pool: &PgPool,
    atom_id: &str,
    reviewer_agent_id: &str,
    decision: &str,
    reason: Option<&str>,
) -> Result<String> {
    let review_id = uuid::Uuid::new_v4().to_string();
    
    // Insert review record
    sqlx::query(
        "INSERT INTO reviews (review_id, atom_id, reviewer_agent_id, decision, reason) 
         VALUES ($1, $2, $3, $4, $5)"
    )
    .bind(&review_id)
    .bind(atom_id)
    .bind(reviewer_agent_id)
    .bind(decision)
    .bind(reason)
    .execute(pool)
    .await?;
    
    // Update atom review status
    let new_status = match decision {
        "approve" => "approved",
        "reject" => "rejected",
        "auto_approve" => "auto_approved",
        _ => return Err(MoteError::Validation("Invalid decision".to_string())),
    };
    
    sqlx::query("UPDATE atoms SET review_status = $1 WHERE atom_id = $2")
        .bind(new_status)
        .bind(atom_id)
        .execute(pool)
        .await?;
    
    // Update agent reliability based on review outcome
    update_agent_reliability_from_review(pool, reviewer_agent_id, atom_id, decision).await?;
    
    Ok(review_id)
}

/// Update agent reliability based on review decisions
async fn update_agent_reliability_from_review(
    pool: &PgPool,
    _reviewer_agent_id: &str,
    atom_id: &str,
    decision: &str,
) -> Result<()> {
    // Get atom author and current reliability
    let row = sqlx::query(
        "SELECT a.author_agent_id, ag.reliability, ag.atoms_published 
         FROM atoms a
         JOIN agents ag ON a.author_agent_id = ag.agent_id
         WHERE a.atom_id = $1"
    )
    .bind(atom_id)
    .fetch_one(pool)
    .await?;
    
    let author_agent_id: String = row.get("author_agent_id");
    let current_reliability: Option<f64> = row.get("reliability");
    let atoms_published: i32 = row.get("atoms_published");
    
    // Initialize reliability if null
    let current_reliability = current_reliability.unwrap_or(0.5);
    
    // Only update reliability if the agent has sufficient publications
    if atoms_published >= 3 {
        let reliability_change = match decision {
            "approve" => 0.05,   // Positive reviews increase reliability
            "reject" => -0.1,   // Rejections decrease reliability more
            _ => 0.0,
        };
        
        let new_reliability = (current_reliability + reliability_change).clamp(0.0, 1.0);
        
        sqlx::query("UPDATE agents SET reliability = $1 WHERE agent_id = $2")
            .bind(new_reliability)
            .bind(&author_agent_id)
            .execute(pool)
            .await?;
    }
    
    Ok(())
}

/// Get review history for an atom
pub async fn get_atom_reviews(
    pool: &PgPool,
    atom_id: &str,
) -> Result<Vec<ReviewRecord>> {
    let rows = sqlx::query(
        "SELECT review_id, atom_id, reviewer_agent_id, decision, reason, created_at
         FROM reviews
         WHERE atom_id = $1
         ORDER BY created_at DESC"
    )
    .bind(atom_id)
    .fetch_all(pool)
    .await?;

    rows.into_iter()
        .map(|row| {
            Ok(ReviewRecord {
                review_id: row.get("review_id"),
                atom_id: row.get("atom_id"),
                reviewer_agent_id: row.get("review_agent_id"),
                decision: row.get("decision"),
                reason: row.get("reason"),
                created_at: row.get("created_at"),
            })
        })
        .collect()
}

// Claim conflict detection functions

#[derive(Debug)]
pub struct ClaimConflict {
    pub claim_id: String,
    pub agent_id: String,
    pub hypothesis: String,
    pub conditions: serde_json::Value,
    pub similarity_score: f64,
    pub conflict_type: String, // "similar_hypothesis", "overlapping_conditions", "competing_direction"
}

/// Find potential claim conflicts based on hypothesis similarity and condition overlap
pub async fn find_potential_claim_conflicts(
    pool: &PgPool,
    domain: &str,
    hypothesis: &str,
    conditions: &serde_json::Value,
) -> Result<Vec<ClaimConflict>> {
    let mut conflicts = Vec::new();
    
    // Get existing claims in the same domain
    let rows = sqlx::query(
        "SELECT c.claim_id, c.agent_id, a.statement, a.conditions
         FROM claims c
         JOIN atoms a ON a.atom_id = c.atom_id
         WHERE c.active = true AND a.domain = $1 AND a.atom_id != c.atom_id"
    )
    .bind(domain)
    .fetch_all(pool)
    .await?;
    
    for row in rows {
        let existing_hypothesis: String = row.get("statement");
        let existing_conditions: serde_json::Value = row.get("conditions");
        
        // Calculate simple text similarity for hypotheses
        let similarity = calculate_text_similarity(hypothesis, &existing_hypothesis);
        
        // Check for condition overlap
        let condition_overlap = calculate_condition_overlap(conditions, &existing_conditions);
        
        // Determine conflict type and severity
        let (conflict_type, should_include) = if similarity > 0.8 {
            ("similar_hypothesis", true)
        } else if condition_overlap > 0.7 {
            ("overlapping_conditions", true)
        } else if similarity > 0.5 && condition_overlap > 0.3 {
            ("competing_direction", true)
        } else {
            ("", false)
        };
        
        if should_include {
            conflicts.push(ClaimConflict {
                claim_id: row.get("claim_id"),
                agent_id: row.get("agent_id"),
                hypothesis: existing_hypothesis,
                conditions: existing_conditions,
                similarity_score: similarity,
                conflict_type: conflict_type.to_string(),
            });
        }
    }
    
    Ok(conflicts)
}

/// Calculate claim density for a domain (claims per unit of research space)
pub async fn calculate_claim_density(
    pool: &PgPool,
    domain: &str,
) -> Result<f64> {
    // Count active claims in domain
    let claim_count: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) 
         FROM claims c
         JOIN atoms a ON a.atom_id = c.atom_id
         WHERE c.active = true AND a.domain = $1"
    )
    .bind(domain)
    .fetch_one(pool)
    .await?;
    
    // Count total atoms in domain for normalization
    let atom_count: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) 
         FROM atoms 
         WHERE domain = $1 AND NOT retracted"
    )
    .bind(domain)
    .fetch_one(pool)
    .await?;
    
    // Calculate density (claims per atom, capped at 1.0)
    let density = if atom_count > 0 {
        (claim_count as f64) / (atom_count as f64)
    } else {
        0.0
    }.min(1.0);
    
    Ok(density)
}

/// Simple text similarity calculation (Jaccard-like)
fn calculate_text_similarity(text1: &str, text2: &str) -> f64 {
    let words1: std::collections::HashSet<&str> = text1.split_whitespace().collect();
    let words2: std::collections::HashSet<&str> = text2.split_whitespace().collect();
    
    if words1.is_empty() && words2.is_empty() {
        return 1.0;
    }
    
    if words1.is_empty() || words2.is_empty() {
        return 0.0;
    }
    
    let intersection = words1.intersection(&words2).count();
    let union = words1.union(&words2).count();
    
    intersection as f64 / union as f64
}

/// Calculate condition overlap between two condition objects
fn calculate_condition_overlap(cond1: &serde_json::Value, cond2: &serde_json::Value) -> f64 {
    if !cond1.is_object() || !cond2.is_object() {
        return 0.0;
    }
    
    let obj1 = cond1.as_object().unwrap();
    let obj2 = cond2.as_object().unwrap();
    
    if obj1.is_empty() && obj2.is_empty() {
        return 1.0;
    }
    
    if obj1.is_empty() || obj2.is_empty() {
        return 0.0;
    }
    
    let mut overlap_count = 0;
    let mut total_keys = 0;
    
    for (key, value1) in obj1 {
        total_keys += 1;
        if let Some(value2) = obj2.get(key) {
            if value1 == value2 {
                overlap_count += 1;
            }
        }
    }
    
    // Also check keys in obj2 that aren't in obj1
    for key in obj2.keys() {
        if !obj1.contains_key(key) {
            total_keys += 1;
        }
    }
    
    if total_keys == 0 {
        0.0
    } else {
        overlap_count as f64 / total_keys as f64
    }
}

// Graph traversal functions

#[derive(Debug, Default)]
pub struct GraphTraversalInfo {
    pub hops_explored: u32,
    pub connected_atoms: Vec<String>,
    pub edge_types_found: Vec<String>,
    pub paths: Vec<Vec<String>>, // Each path is a sequence of atom_ids
}

/// Get graph traversal information for a set of atoms
pub async fn get_graph_traversal_info(
    pool: &PgPool,
    atom_ids: &[String],
    max_hops: u32,
    edge_types_filter: Option<&[String]>,
) -> Result<GraphTraversalInfo> {
    let mut connected_atoms = Vec::new();
    let mut edge_types_found = std::collections::HashSet::new();
    let mut paths = Vec::new();
    
    // Build edge type filter clause
    let edge_type_clause = if let Some(filter) = edge_types_filter {
        let placeholders: Vec<String> = filter.iter().map(|_| "?".to_string()).collect();
        format!("AND e.type IN ({})", placeholders.join(","))
    } else {
        String::new()
    };
    
    // Find connected atoms within max_hops
    for atom_id in atom_ids {
        let query = format!(
            "WITH RECURSIVE connected_atoms(atom_id, hop, path) AS (
                SELECT target_id, 1, ARRAY[target_id] 
                FROM edges 
                WHERE source_id = $1 {}
                UNION ALL
                SELECT e.target_id, ca.hop + 1, ca.path || e.target_id
                FROM edges e
                JOIN connected_atoms ca ON e.source_id = ca.atom_id
                WHERE ca.hop < {}
                AND NOT e.target_id = ANY(ca.path)
                {}
            )
            SELECT DISTINCT atom_id, hop, path
            FROM connected_atoms",
            edge_type_clause, max_hops, edge_type_clause
        );
        
        let mut query_builder = sqlx::query(&query).bind(atom_id);
        
        // Add edge type filter values if provided
        if let Some(filter) = edge_types_filter {
            for edge_type in filter {
                query_builder = query_builder.bind(edge_type);
            }
        }
        
        let rows = query_builder
            .fetch_all(pool)
            .await?;
        
        for row in rows {
            let connected_id: String = row.get("atom_id");
            let path: Vec<String> = row.get("path");
            
            if !connected_atoms.contains(&connected_id) {
                connected_atoms.push(connected_id);
            }
            
            if !paths.contains(&path) {
                paths.push(path);
            }
        }
    }
    
    // Get edge types found
    let edge_query = if let Some(filter) = edge_types_filter {
        let placeholders: Vec<String> = filter.iter().map(|_| "?".to_string()).collect();
        format!(
            "SELECT DISTINCT type FROM edges 
             WHERE (source_id = ANY($1) OR target_id = ANY($1))
             AND type IN ({})",
            placeholders.join(",")
        )
    } else {
        "SELECT DISTINCT type FROM edges WHERE (source_id = ANY($1) OR target_id = ANY($1))".to_string()
    };
    
    let mut edge_query_builder = sqlx::query(&edge_query).bind(&connected_atoms);
    
    if let Some(filter) = edge_types_filter {
        for edge_type in filter {
            edge_query_builder = edge_query_builder.bind(edge_type);
        }
    }
    
    let edge_rows = edge_query_builder
        .fetch_all(pool)
        .await?;
    
    for row in edge_rows {
        let edge_type: String = row.get("type");
        edge_types_found.insert(edge_type);
    }
    
    Ok(GraphTraversalInfo {
        hops_explored: max_hops,
        connected_atoms,
        edge_types_found: edge_types_found.into_iter().collect(),
        paths,
    })
}
