use crate::error::Result;
use sqlx::{PgPool, Row};

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
