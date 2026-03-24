use crate::domain::atom::Lifecycle;
use crate::domain::lifecycle::{AtomState, LifecycleEvaluator, LifecycleTransition};
use crate::metrics::emergence::EmergenceMetrics;
use crate::state::SseEvent;
use sqlx::{PgPool, Row};
use std::time::Duration;
use tokio::sync::broadcast;
use tokio_util::sync::CancellationToken;
use tracing::{error, info};

pub struct LifecycleWorker {
    pool: PgPool,
    evaluator: LifecycleEvaluator,
    cancel_token: CancellationToken,
    sse_tx: broadcast::Sender<SseEvent>,
    interval_minutes: u64,
}

impl LifecycleWorker {
    pub fn new(
        pool: PgPool,
        evaluator: LifecycleEvaluator,
        cancel_token: CancellationToken,
        sse_tx: broadcast::Sender<SseEvent>,
        interval_minutes: u64,
    ) -> Self {
        Self {
            pool,
            evaluator,
            cancel_token,
            sse_tx,
            interval_minutes,
        }
    }

    pub async fn start(self) {
        info!("Starting lifecycle worker (interval: {}m)", self.interval_minutes);
        let mut interval =
            tokio::time::interval(Duration::from_secs(self.interval_minutes * 60));
        loop {
            tokio::select! {
                _ = interval.tick() => {
                    self.run_lifecycle_sweep().await;
                }
                _ = self.cancel_token.cancelled() => {
                    info!("Lifecycle worker shutting down");
                    break;
                }
            }
        }
    }

    async fn run_lifecycle_sweep(&self) {
        let rows = match sqlx::query(
            "SELECT
                a.atom_id,
                a.lifecycle,
                a.repl_exact,
                a.ph_disagreement::float8 as ph_disagreement,
                (SELECT COUNT(*) FROM edges
                 WHERE (source_id = a.atom_id OR target_id = a.atom_id)
                   AND type = 'contradicts') as contradicts_edge_count,
                (SELECT COUNT(*) FROM edges
                 WHERE target_id = a.atom_id AND type = 'replicates') as replicates_edge_count
             FROM atoms a
             WHERE a.lifecycle IN ('provisional', 'replicated', 'contested')
               AND NOT a.archived
               AND NOT a.retracted"
        )
        .fetch_all(&self.pool)
        .await
        {
            Ok(rows) => rows,
            Err(e) => {
                error!("Lifecycle sweep query failed: {}", e);
                return;
            }
        };

        let mut transitioned = 0usize;
        for row in rows {
            let lifecycle_str: String = row.get("lifecycle");
            let lifecycle = match lifecycle_str.as_str() {
                "provisional" => Lifecycle::Provisional,
                "replicated"  => Lifecycle::Replicated,
                "core"        => Lifecycle::Core,
                "contested"   => Lifecycle::Contested,
                "resolved"    => Lifecycle::Resolved,
                "retracted"   => Lifecycle::Retracted,
                _             => continue,
            };
            let repl_exact: i32 = row.get("repl_exact");
            let ph_disagreement: f64 = row.get("ph_disagreement");
            let contradicts_edge_count: i64 = row.get("contradicts_edge_count");
            let replicates_edge_count: i64 = row.get("replicates_edge_count");
            let atom = AtomState {
                atom_id: row.get("atom_id"),
                lifecycle,
                repl_exact,
                ph_disagreement,
                contradicts_edge_count,
                replicates_edge_count,
            };
            if let Some(transition) = self.evaluator.evaluate(&atom) {
                self.apply_transition(&atom.atom_id, transition).await;
                transitioned += 1;
            }
        }

        if transitioned > 0 {
            info!("Lifecycle sweep: {} atoms transitioned", transitioned);
        }
    }

    async fn apply_transition(&self, atom_id: &str, transition: LifecycleTransition) {
        // ToRetracted is handled by the API, not the worker
        if transition == LifecycleTransition::ToRetracted {
            return;
        }

        let new_lifecycle = transition.new_lifecycle_str();

        // Fetch current lifecycle for the audit record
        let old_lifecycle: Option<String> = sqlx::query_scalar(
            "SELECT lifecycle FROM atoms WHERE atom_id = $1"
        )
        .bind(atom_id)
        .fetch_optional(&self.pool)
        .await
        .ok()
        .flatten();

        match sqlx::query("UPDATE atoms SET lifecycle = $1 WHERE atom_id = $2")
            .bind(new_lifecycle)
            .bind(atom_id)
            .execute(&self.pool)
            .await
        {
            Ok(_) => {
                // Record to audit table for metrics
                if let Some(from) = old_lifecycle {
                    EmergenceMetrics::record_transition(&self.pool, atom_id, &from, new_lifecycle).await;
                }

                let _ = self.sse_tx.send(SseEvent {
                    event_type: "lifecycle_transition".to_string(),
                    data: serde_json::json!({
                        "atom_id": atom_id,
                        "new_lifecycle": new_lifecycle,
                    }),
                    timestamp: chrono::Utc::now(),
                });
                info!("Atom {} transitioned to {}", atom_id, new_lifecycle);
            }
            Err(e) => {
                error!("Failed to apply lifecycle transition for {}: {}", atom_id, e);
            }
        }
    }
}

