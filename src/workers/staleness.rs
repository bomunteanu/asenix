use pgvector::Vector;
use sqlx::{PgPool, Row};
use tokio::sync::broadcast;
use tracing::{info, error, debug};
use std::time::Duration;

use crate::state::SseEvent;

pub struct StalenessWorker {
    pool: PgPool,
    neighbourhood_radius: f64,
    staleness_threshold: usize,
    bounty_threshold: f64,
    sse_tx: broadcast::Sender<SseEvent>,
}

impl StalenessWorker {
    pub fn new(
        pool: PgPool,
        neighbourhood_radius: f64,
        bounty_threshold: f64,
        sse_tx: broadcast::Sender<SseEvent>,
    ) -> Self {
        Self {
            pool,
            neighbourhood_radius,
            staleness_threshold: 20,
            bounty_threshold,
            sse_tx,
        }
    }

    /// Run staleness check and emit synthesis_needed events
    pub async fn run_staleness_check(&self) -> Result<usize, sqlx::Error> {
        // Get all synthesis atoms that are not retracted and have ready embeddings
        let synthesis_atoms = sqlx::query(
            "SELECT atom_id, embedding, created_at, domain FROM atoms
             WHERE type = 'synthesis' AND NOT retracted AND embedding_status = 'ready'"
        )
        .fetch_all(&self.pool)
        .await?;

        let mut stale_count = 0;

        for synthesis_row in synthesis_atoms {
            let atom_id: String = synthesis_row.get("atom_id");
            let embedding: Option<Vector> = synthesis_row.get("embedding");
            let created_at: chrono::DateTime<chrono::Utc> = synthesis_row.get("created_at");
            let domain: String = synthesis_row.get("domain");

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
            .bind(embedding.as_ref())
            .bind(self.neighbourhood_radius)
            .fetch_one(&self.pool)
            .await?;

            if newer_atoms_count as usize > self.staleness_threshold {
                info!(
                    "Stale synthesis detected: {} has {} newer atoms in neighbourhood (threshold: {})",
                    atom_id, newer_atoms_count, self.staleness_threshold
                );
                
                // Emit synthesis_needed event
                if let Some(ref emb) = embedding {
                    self.emit_synthesis_needed_event(emb.as_slice(), newer_atoms_count as usize, &domain);
                }
                stale_count += 1;
            }
        }

        if stale_count > 0 {
            info!("Detected {} stale synthesis regions", stale_count);
        }

        Ok(stale_count)
    }

    /// Run bounty check and emit bounty_needed events
    pub async fn run_bounty_check(&self, threshold: f64) -> Result<usize, Box<dyn std::error::Error + Send + Sync>> {
        // Get domain novelty statistics
        let domain_stats = crate::db::queries::get_domain_novelty_stats(&self.pool).await?;
        
        let mut bounty_count = 0;
        
        for (domain, mean_novelty) in domain_stats {
            if mean_novelty > threshold {
                info!(
                    "High novelty detected in domain {}: mean_novelty={:.3} (threshold: {:.3})",
                    domain, mean_novelty, threshold
                );
                
                // Emit bounty_needed event
                self.emit_bounty_needed_event(&domain, mean_novelty);
                bounty_count += 1;
            }
        }

        if bounty_count > 0 {
            info!("Detected {} domains needing bounties", bounty_count);
        }

        Ok(bounty_count)
    }

    /// Emit synthesis_needed event to the SSE broadcast channel.
    fn emit_synthesis_needed_event(&self, cluster_center: &[f32], atom_count: usize, domain: &str) {
        let event = SseEvent {
            event_type: "synthesis_needed".to_string(),
            data: serde_json::json!({
                "type": "synthesis_needed",
                "cluster_center": cluster_center,
                "atom_count": atom_count,
                "domain": domain,
            }),
            timestamp: chrono::Utc::now(),
        };
        // SendError only occurs when there are no receivers — safe to ignore.
        let _ = self.sse_tx.send(event);
        debug!(
            "Emitted synthesis_needed event: domain={}, atom_count={}",
            domain, atom_count
        );
    }

    /// Emit bounty_needed event to the SSE broadcast channel.
    fn emit_bounty_needed_event(&self, domain: &str, mean_novelty: f64) {
        let event = SseEvent {
            event_type: "bounty_needed".to_string(),
            data: serde_json::json!({
                "type": "bounty_needed",
                "domain": domain,
                "mean_novelty": mean_novelty,
                "timestamp": chrono::Utc::now().to_rfc3339()
            }),
            timestamp: chrono::Utc::now(),
        };
        // SendError only occurs when there are no receivers — safe to ignore.
        let _ = self.sse_tx.send(event);
        debug!(
            "Emitted bounty_needed event: domain={}, mean_novelty={}",
            domain, mean_novelty
        );
    }

    /// Start the periodic staleness worker
    pub async fn start(self, interval_minutes: u64) {
        let mut interval = tokio::time::interval(Duration::from_secs(interval_minutes * 60));

        loop {
            tokio::select! {
                _ = interval.tick() => {
                    // Run staleness check
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
                    
                    // Run bounty check
                    match self.run_bounty_check(self.bounty_threshold).await {
                        Ok(bounty_count) => {
                            if bounty_count > 0 {
                                info!("Bounty check completed: {} domains need bounties", bounty_count);
                            }
                        }
                        Err(e) => {
                            error!("Bounty check failed: {}", e);
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
