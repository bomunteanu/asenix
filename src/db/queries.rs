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
