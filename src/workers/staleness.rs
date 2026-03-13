use sqlx::{PgPool, Row};
use tokio::sync::broadcast;
use tracing::{info, error, debug};
use std::time::Duration;

use crate::state::SseEvent;

pub struct StalenessWorker {
    pool: PgPool,
    neighbourhood_radius: f64,
    staleness_threshold: usize,
    sse_tx: broadcast::Sender<SseEvent>,
}

impl StalenessWorker {
    pub fn new(
        pool: PgPool,
        neighbourhood_radius: f64,
        sse_tx: broadcast::Sender<SseEvent>,
    ) -> Self {
        Self {
            pool,
            neighbourhood_radius,
            staleness_threshold: 20,
            sse_tx,
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
                    self.emit_synthesis_needed_event(&embedding, newer_atoms_count as usize);
                }
                stale_count += 1;
            }
        }

        if stale_count > 0 {
            info!("Detected {} stale synthesis regions", stale_count);
        }

        Ok(stale_count)
    }

    /// Emit synthesis_needed event to the SSE broadcast channel.
    fn emit_synthesis_needed_event(&self, cluster_center: &[f64], atom_count: usize) {
        let event = SseEvent {
            event_type: "synthesis_needed".to_string(),
            data: serde_json::json!({
                "type": "synthesis_needed",
                "cluster_center": cluster_center,
                "atom_count": atom_count,
            }),
            timestamp: chrono::Utc::now(),
        };
        // SendError only occurs when there are no receivers — safe to ignore.
        let _ = self.sse_tx.send(event);
        debug!(
            "Emitted synthesis_needed event: atom_count={}",
            atom_count
        );
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
