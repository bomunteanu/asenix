use crate::error::Result;
use sqlx::{PgPool, Row};

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
