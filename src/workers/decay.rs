use crate::config::Config;
use crate::domain::pheromone::decay_attraction;
use crate::error::{MoteError, Result};
use sqlx::{PgPool, Row};
use tracing::info;
use chrono::{DateTime, Utc, Duration};

pub struct DecayWorker {
    pool: PgPool,
    config: Config,
}

impl DecayWorker {
    pub fn new(pool: PgPool, config: Config) -> Self {
        Self { pool, config }
    }

    /// Run a single decay sweep on all atoms with positive attraction
    pub async fn run_decay_sweep(&self) -> Result<usize> {
        let start_time = Utc::now();
        
        // Select atoms with positive attraction that are not archived
        let rows = sqlx::query(
            "SELECT atom_id, ph_attraction, created_at 
             FROM atoms 
             WHERE ph_attraction > 0.001 
             AND NOT archived"
        )
        .fetch_all(&self.pool)
        .await?;

        if rows.is_empty() {
            info!("No atoms require attraction decay");
            return Ok(0);
        }

        let mut decayed_atoms = Vec::new();
        let floor_threshold = 0.001;
        let half_life_hours = self.config.pheromone.decay_half_life_hours as f64;

        for row in rows {
            let atom_id: String = row.get("atom_id");
            let current_attraction: f64 = row.get("ph_attraction");
            let created_at: DateTime<Utc> = row.get("created_at");

            // Calculate hours elapsed since creation (MVP simplification)
            let hours_elapsed = Utc::now().signed_duration_since(created_at).num_hours() as f64;
            
            // Apply decay
            let decayed_attraction = decay_attraction(
                current_attraction,
                hours_elapsed,
                half_life_hours,
                floor_threshold,
            );

            // Only update if value changed significantly
            if (decayed_attraction - current_attraction).abs() > floor_threshold {
                decayed_atoms.push((atom_id, decayed_attraction));
            }
        }

        if decayed_atoms.is_empty() {
            info!("No atoms had significant attraction decay");
            return Ok(0);
        }

        // Apply batch updates
        let updated_count = self.apply_decay_updates(decayed_atoms).await?;

        let elapsed = (Utc::now() - start_time).num_milliseconds();
        info!(
            "Decay sweep completed: {} atoms updated in {}ms", 
            updated_count, 
            elapsed
        );

        Ok(updated_count)
    }

    /// Apply decay updates in a single batch operation
    async fn apply_decay_updates(&self, decayed_atoms: Vec<(String, f64)>) -> Result<usize> {
        if decayed_atoms.is_empty() {
            return Ok(0);
        }

        let count = decayed_atoms.len();

        // For MVP, use individual updates in a transaction
        // In a production system, this would use a more efficient batch operation
        let mut tx = self.pool.begin().await.map_err(|e| MoteError::Database(e))?;

        for (atom_id, new_attraction) in decayed_atoms {
            sqlx::query("UPDATE atoms SET ph_attraction = $1 WHERE atom_id = $2")
                .bind(new_attraction)
                .bind(&atom_id)
                .execute(&mut *tx)
                .await
                .map_err(|e| MoteError::Database(e))?;
        }

        tx.commit().await.map_err(|e| MoteError::Database(e))?;

        Ok(count)
    }

    /// Get decay statistics for monitoring
    pub async fn get_decay_stats(&self) -> Result<DecayStats> {
        let row = sqlx::query(
            "SELECT 
                COUNT(*) as total_atoms,
                COUNT(*) FILTER (WHERE ph_attraction > 0.001) as atoms_with_attraction,
                AVG(ph_attraction) FILTER (WHERE ph_attraction > 0) as avg_attraction,
                MAX(ph_attraction) as max_attraction
             FROM atoms 
             WHERE NOT archived"
        )
        .fetch_one(&self.pool)
        .await?;

        let total_atoms: i64 = row.get("total_atoms");
        let atoms_with_attraction: i64 = row.get("atoms_with_attraction");
        let avg_attraction: Option<f64> = row.get("avg_attraction");
        let max_attraction: Option<f64> = row.get("max_attraction");

        Ok(DecayStats {
            total_atoms: total_atoms as usize,
            atoms_with_attraction: atoms_with_attraction as usize,
            avg_attraction: avg_attraction.unwrap_or(0.0),
            max_attraction: max_attraction.unwrap_or(0.0),
        })
    }
}

#[derive(Debug, Clone)]
pub struct DecayStats {
    pub total_atoms: usize,
    pub atoms_with_attraction: usize,
    pub avg_attraction: f64,
    pub max_attraction: f64,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::pheromone::decay_attraction;

    #[test]
    fn test_decay_calculation() {
        // Test basic decay
        let result = decay_attraction(10.0, 168.0, 168.0, 0.001); // 1 half-life
        assert!((result - 5.0).abs() < f32::EPSILON as f64);

        // Test below floor
        let result = decay_attraction(0.0005, 168.0, 168.0, 0.001);
        assert_eq!(result, 0.0);

        // Test no decay
        let result = decay_attraction(10.0, 0.0, 168.0, 0.001);
        assert_eq!(result, 10.0);
    }

    #[test]
    fn test_decay_stats_structure() {
        let stats = DecayStats {
            total_atoms: 100,
            atoms_with_attraction: 50,
            avg_attraction: 2.5,
            max_attraction: 10.0,
        };

        assert_eq!(stats.total_atoms, 100);
        assert_eq!(stats.atoms_with_attraction, 50);
        assert_eq!(stats.avg_attraction, 2.5);
        assert_eq!(stats.max_attraction, 10.0);
    }
}
