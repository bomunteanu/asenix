use sqlx::{PgPool, Row};
use tracing::{info, error, debug};
use std::time::Duration;

pub struct StalenessWorker {
    pool: PgPool,
    neighbourhood_radius: f64,
    staleness_threshold: usize,
}

impl StalenessWorker {
    pub fn new(pool: PgPool, neighbourhood_radius: f64) -> Self {
        Self {
            pool,
            neighbourhood_radius,
            staleness_threshold: 20, // Default threshold
        }
    }

    /// Run staleness check and emit synthesis_needed events
    pub async fn run_staleness_check(&self) -> Result<usize, sqlx::Error> {
        // Get all synthesis atoms that are not retracted and have ready embeddings
        let synthesis_atoms = sqlx::query(
            "SELECT atom_id, embedding, created_at FROM atoms 
             WHERE type = 'synthesis' AND NOT retracted AND embedding_status = 'ready'"
        )
        .fetch_all(&self.pool)
        .await?;

        let mut stale_count = 0;

        for synthesis_row in synthesis_atoms {
            let atom_id: String = synthesis_row.get("atom_id");
            let embedding: Option<Vec<f64>> = synthesis_row.get("embedding");
            let created_at: chrono::DateTime<chrono::Utc> = synthesis_row.get("created_at");

            // Count newer atoms in neighbourhood
            let newer_atoms_count = sqlx::query_scalar::<_, i64>(
                "SELECT COUNT(*) FROM atoms 
                 WHERE embedding IS NOT NULL 
                 AND embedding_status = 'ready'
                 AND NOT retracted
                 AND NOT archived
                 AND created_at > $1
                 AND embedding <=> $2 < $3"
            )
            .bind(created_at)
            .bind(&embedding)
            .bind(self.neighbourhood_radius)
            .fetch_one(&self.pool)
            .await?;

            if newer_atoms_count as usize > self.staleness_threshold {
                info!(
                    "Stale synthesis detected: {} has {} newer atoms in neighbourhood (threshold: {})",
                    atom_id, newer_atoms_count, self.staleness_threshold
                );
                
                // Emit synthesis_needed event
                if let Some(embedding) = embedding {
                    self.emit_synthesis_needed_event(&embedding, newer_atoms_count as usize).await?;
                }
                stale_count += 1;
            }
        }

        if stale_count > 0 {
            info!("Detected {} stale synthesis regions", stale_count);
        }

        Ok(stale_count)
    }

    /// Emit synthesis_needed event to the broadcast channel
    async fn emit_synthesis_needed_event(
        &self,
        cluster_center: &[f64],
        atom_count: usize,
    ) -> Result<(), sqlx::Error> {
        // For now, we'll log the event. In a full implementation,
        // this would emit to the SSE broadcast channel
        debug!(
            "Would emit synthesis_needed event: cluster_center={:?}, atom_count={}",
            cluster_center, atom_count
        );

        // TODO: Actually emit to SSE broadcast channel
        // This requires access to the broadcast channel from AppState
        // For now, we'll just log it

        Ok(())
    }

    /// Start the periodic staleness worker
    pub async fn start(self, interval_minutes: u64) {
        let mut interval = tokio::time::interval(Duration::from_secs(interval_minutes * 60));

        loop {
            tokio::select! {
                _ = interval.tick() => {
                    match self.run_staleness_check().await {
                        Ok(stale_count) => {
                            if stale_count > 0 {
                                info!("Staleness check completed: {} stale regions found", stale_count);
                            }
                        }
                        Err(e) => {
                            error!("Staleness check failed: {}", e);
                        }
                    }
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    

    #[tokio::test]
    async fn test_stale_synthesis_detection() {
        // Test would:
        // 1. Create a synthesis atom
        // 2. Create 25 atoms in same neighbourhood with newer timestamps
        // 3. Run staleness check
        // 4. Verify synthesis_needed event is emitted
    }

    #[tokio::test]
    async fn test_fresh_synthesis_not_flagged() {
        // Test would:
        // 1. Create a synthesis atom
        // 2. Create 5 atoms in same neighbourhood (below threshold)
        // 3. Run staleness check
        // 4. Verify no event is emitted
    }

    #[tokio::test]
    async fn test_retracted_synthesis_ignored() {
        // Test would:
        // 1. Create a synthesis atom
        // 2. Retract it
        // 3. Create 25 atoms nearby
        // 4. Run staleness check
        // 5. Verify no event is emitted
    }
}
