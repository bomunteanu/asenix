use crate::config::Config;
use crate::domain::pheromone::decay_attraction;
use crate::error::{MoteError, Result};
use sqlx::{PgPool, Row};
use std::time::Duration;
use tokio_util::sync::CancellationToken;
use tracing::{error, info};

pub struct DecayWorker {
    pool: PgPool,
    config: Config,
}

impl DecayWorker {
    pub fn new(pool: PgPool, config: Config) -> Self {
        Self { pool, config }
    }

    /// Run a single decay sweep on all atoms with positive attraction.
    ///
    /// Decay is measured in atoms published in the same domain since this atom's
    /// last_activity_at — so a busy domain erodes signals faster than a quiet one.
    pub async fn run_decay_sweep(&self) -> Result<usize> {
        // Single query: for each atom with positive attraction, count how many
        // newer atoms exist in the same domain (i.e. domain activity since last touch).
        let rows = sqlx::query(
            "SELECT a.atom_id,
                    a.ph_attraction,
                    COUNT(newer.atom_id) AS atoms_since
             FROM atoms a
             LEFT JOIN atoms newer
               ON newer.domain = a.domain
              AND newer.created_at > a.last_activity_at
              AND NOT newer.archived
              AND NOT newer.retracted
             WHERE a.ph_attraction > 0.001
               AND NOT a.archived
             GROUP BY a.atom_id, a.ph_attraction"
        )
        .fetch_all(&self.pool)
        .await?;

        if rows.is_empty() {
            info!("No atoms require attraction decay");
            return Ok(0);
        }

        let mut decayed_atoms = Vec::new();
        let floor_threshold = 0.001_f64;
        let half_life = self.config.pheromone.decay_half_life_atoms as f64;

        for row in rows {
            let atom_id: String = row.get("atom_id");
            let current: f64 = row.get::<f32, _>("ph_attraction") as f64;
            let atoms_since: i64 = row.get("atoms_since");

            let decayed = decay_attraction(current, atoms_since as f64, half_life, floor_threshold);

            if (decayed - current).abs() > floor_threshold {
                decayed_atoms.push((atom_id, decayed));
            }
        }

        if decayed_atoms.is_empty() {
            info!("No atoms had significant attraction decay");
            return Ok(0);
        }

        let updated_count = self.apply_decay_updates(decayed_atoms).await?;
        info!("Decay sweep completed: {} atoms updated", updated_count);
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
        let mut tx = self.pool.begin().await.map_err(MoteError::Database)?;

        for (atom_id, new_attraction) in decayed_atoms {
            sqlx::query("UPDATE atoms SET ph_attraction = $1 WHERE atom_id = $2")
                .bind(new_attraction)
                .bind(&atom_id)
                .execute(&mut *tx)
                .await
                .map_err(MoteError::Database)?;
        }

        tx.commit().await.map_err(MoteError::Database)?;

        Ok(count)
    }

    /// Start the periodic decay worker loop with cooperative shutdown.
    pub async fn start(self, cancel_token: CancellationToken) {
        let base = Duration::from_secs(self.config.workers.decay_interval_minutes * 60);
        loop {
            let sleep_dur = jittered_duration(base);
            tokio::select! {
                _ = tokio::time::sleep(sleep_dur) => {
                    if let Err(e) = self.run_decay_sweep().await {
                        error!("Decay worker sweep failed: {}", e);
                    }
                }
                _ = cancel_token.cancelled() => {
                    info!("Decay worker shutting down");
                    break;
                }
            }
        }
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
        let max_attraction: Option<f64> = row.get::<Option<f32>, _>("max_attraction").map(|v| v as f64);

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

fn jittered_duration(base: Duration) -> Duration {
    use rand::{Rng, SeedableRng};
    let mut rng = rand::rngs::StdRng::from_os_rng();
    let factor: f64 = 0.8 + rng.random::<f64>() * 0.4;
    Duration::from_secs_f64(base.as_secs_f64() * factor)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::pheromone::decay_attraction;

    #[test]
    fn test_decay_calculation() {
        // One half-life worth of domain activity → attraction halves
        let result = decay_attraction(10.0, 50.0, 50.0, 0.001);
        assert!((result - 5.0).abs() < f32::EPSILON as f64);

        // Below floor → zeroed out
        let result = decay_attraction(0.0005, 50.0, 50.0, 0.001);
        assert_eq!(result, 0.0);

        // Zero new atoms → no decay
        let result = decay_attraction(10.0, 0.0, 50.0, 0.001);
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
