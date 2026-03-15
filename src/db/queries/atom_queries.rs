use crate::error::{MoteError, Result};
use crate::domain::atom::{Atom, AtomInput, AtomType};
use sqlx::{PgPool, Row};
use crate::crypto::hashing::compute_atom_id;

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
            Some(_) => {
                // Artifact exists — both blobs and trees are valid attachments
            }
            None => {
                return Err(MoteError::Validation(
                    format!("Artifact {} does not exist. Upload the artifact first.", artifact_hash)
                ));
            }
        }
    }

    let mut tx = pool.begin().await.map_err(MoteError::Database)?;
    
    sqlx::query(
        "INSERT INTO atoms (atom_id, type, domain, project_id, statement, conditions, metrics, provenance, signature, author_agent_id, created_at, embedding_status, lifecycle, retracted, artifact_tree_hash, review_status)
         VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, NOW(), 'pending', 'provisional', false, $11,
           CASE WHEN EXISTS(
             SELECT 1 FROM agents
             WHERE agent_id = $10 AND reliability >= 0.8 AND atoms_published >= 5
           ) THEN 'auto_approved' ELSE 'pending' END
         )"
    )
    .bind(&atom_id)
    .bind(atom_input.atom_type.to_string())
    .bind(&atom_input.domain)
    .bind(&atom_input.project_id)
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

    sqlx::query("UPDATE agents SET atoms_published = atoms_published + 1 WHERE agent_id = $1")
        .bind(agent_id)
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
    project_id_filter: Option<&str>,
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

    if let Some(_project_id) = project_id_filter {
        bind_count += 1;
        query.push_str(&format!(" AND project_id = ${}", bind_count));
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
    if let Some(project_id) = project_id_filter {
        query_builder = query_builder.bind(project_id);
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
            project_id: row.get("project_id"),
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
            project_id: row.get("project_id"),
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

/// Admin ban: marks atom as retracted and sets ban_flag. No author check.
pub async fn ban_atom(pool: &PgPool, atom_id: &str) -> Result<()> {
    sqlx::query("UPDATE atoms SET retracted = true, ban_flag = true, retraction_reason = 'banned' WHERE atom_id = $1")
        .bind(atom_id)
        .execute(pool)
        .await
        .map_err(MoteError::Database)?;
    Ok(())
}

/// Admin unban: clears retracted and ban_flag.
pub async fn unban_atom(pool: &PgPool, atom_id: &str) -> Result<()> {
    sqlx::query("UPDATE atoms SET retracted = false, ban_flag = false, retraction_reason = NULL WHERE atom_id = $1")
        .bind(atom_id)
        .execute(pool)
        .await
        .map_err(MoteError::Database)?;
    Ok(())
}

// Helper function to create Atom from database row
pub fn atom_from_row(row: sqlx::postgres::PgRow) -> Result<Atom> {
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
        project_id: row.get("project_id"),
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
