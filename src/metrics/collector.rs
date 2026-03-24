use crate::metrics::emergence::EmergenceMetrics;
use sqlx::PgPool;
use std::time::Duration;
use tokio_util::sync::CancellationToken;
use tracing::{error, info};

// k is stored per-collector so it can be read from config at startup.
// It must stay constant for the lifetime of a sweep (see diversity.rs docs).

pub struct MetricsCollector {
    pool: PgPool,
    interval: Duration,
    cancel_token: CancellationToken,
    /// Fixed k for frontier diversity clustering. Set once from config at startup.
    frontier_diversity_k: usize,
}

impl MetricsCollector {
    pub fn new(
        pool: PgPool,
        interval_secs: u64,
        cancel_token: CancellationToken,
        frontier_diversity_k: usize,
    ) -> Self {
        Self {
            pool,
            interval: Duration::from_secs(interval_secs),
            cancel_token,
            frontier_diversity_k,
        }
    }

    pub async fn start(self) {
        info!(
            "Starting metrics collector (interval: {}s)",
            self.interval.as_secs()
        );
        let metrics = EmergenceMetrics::new(self.pool.clone());
        let mut ticker = tokio::time::interval(self.interval);

        loop {
            tokio::select! {
                _ = ticker.tick() => {
                    if let Err(e) = self.collect_and_store(&metrics).await {
                        error!("Metrics collection failed: {}", e);
                    }
                }
                _ = self.cancel_token.cancelled() => {
                    info!("Metrics collector shutting down");
                    break;
                }
            }
        }
    }

    async fn collect_and_store(&self, metrics: &EmergenceMetrics) -> Result<(), sqlx::Error> {
        // Collect all 5 metrics concurrently
        let window = Duration::from_secs(3600); // 1-hour rolling window

        let k = self.frontier_diversity_k;
        let (crystallization, diversity, contradiction, landscape, propagation) = tokio::join!(
            metrics.crystallization_rate(window),
            metrics.frontier_diversity(window, k),
            metrics.contradiction_resolution(),
            metrics.landscape_structure(),
            metrics.information_propagation(),
        );

        // Count current agents and atoms
        let agent_count: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM agents WHERE confirmed = true"
        )
        .fetch_one(&self.pool)
        .await
        .unwrap_or(0);

        let atom_count: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM atoms WHERE NOT archived AND NOT retracted"
        )
        .fetch_one(&self.pool)
        .await
        .unwrap_or(0);

        sqlx::query(
            "INSERT INTO metrics_snapshots
                (agent_count, atom_count, crystallization_rate, frontier_diversity,
                 contradiction_resolution, landscape_structure, information_propagation)
             VALUES ($1, $2, $3, $4, $5, $6, $7)"
        )
        .bind(agent_count as i32)
        .bind(atom_count as i32)
        .bind(serde_json::to_value(&crystallization).unwrap_or_default())
        .bind(serde_json::to_value(&diversity).unwrap_or_default())
        .bind(serde_json::to_value(&contradiction).unwrap_or_default())
        .bind(serde_json::to_value(&landscape).unwrap_or_default())
        .bind(serde_json::to_value(&propagation).unwrap_or_default())
        .execute(&self.pool)
        .await?;

        info!(
            "Metrics snapshot stored (agents={}, atoms={}, diversity_entropy={:.3}, diversity_k={}, crystallization={})",
            agent_count, atom_count, diversity.entropy, diversity.k, crystallization.transitions_in_window
        );

        Ok(())
    }
}
