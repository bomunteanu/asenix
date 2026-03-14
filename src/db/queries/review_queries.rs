use crate::error::{MoteError, Result};
use sqlx::{PgPool, Row};

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
