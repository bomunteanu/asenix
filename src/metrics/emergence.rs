use crate::metrics::diversity::{compute_frontier_diversity, FrontierDiversityData};
use chrono::{DateTime, Utc};
use serde::Serialize;
use sqlx::{PgPool, Row};
use std::time::Duration;
use tracing::error;


pub struct EmergenceMetrics {
    pub pool: PgPool,
}

// ─── Output types ─────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize)]
pub struct CrystallizationData {
    /// Number of atoms that reached core/resolved within the window
    pub transitions_in_window: i64,
    /// Average hours from atom creation to reaching core/resolved
    pub avg_hours_to_core: f64,
    /// Transitions per hour over the measurement window
    pub rate_per_hour: f64,
}

#[derive(Debug, Clone, Serialize)]
pub struct ContradictionData {
    /// Atoms that moved contested → resolved (all time)
    pub resolved_count: i64,
    /// Atoms currently in contested state
    pub contested_count: i64,
    /// Average hours from creation to resolution (for resolved atoms)
    pub avg_hours_to_resolve: f64,
    /// Fraction of ever-contested atoms that have been resolved
    pub resolution_rate: f64,
}

#[derive(Debug, Clone, Serialize)]
pub struct LandscapeData {
    /// Variance of ph_attraction across all active atoms
    pub ph_attraction_variance: f64,
    /// Mean ph_attraction
    pub ph_attraction_mean: f64,
    /// Variance of ph_repulsion
    pub ph_repulsion_variance: f64,
    /// Coefficient of variation (std / mean) — measures structure vs noise
    pub coefficient_of_variation: f64,
    pub atom_count: i64,
}

#[derive(Debug, Clone, Serialize)]
pub struct PropagationData {
    /// Median hours between parent atom creation and child atom creation
    pub median_citation_lag_hours: f64,
    /// Total citation edges counted
    pub citation_count: i64,
    /// Number of distinct active agents at measurement time
    pub agent_count: i64,
}

// ─── Metric implementations ───────────────────────────────────────────────────

impl EmergenceMetrics {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }

    /// Knowledge crystallization rate: how fast atoms reach core/resolved status.
    /// Measured by querying lifecycle_transitions within the window.
    pub async fn crystallization_rate(&self, window: Duration) -> CrystallizationData {
        let cutoff: DateTime<Utc> = Utc::now() - chrono::Duration::from_std(window).unwrap_or_default();
        let window_hours = window.as_secs_f64() / 3600.0;

        let row = sqlx::query(
            "SELECT
                COUNT(*) as transitions_in_window,
                COALESCE(AVG(
                    EXTRACT(EPOCH FROM (t.transitioned_at - a.created_at)) / 3600.0
                )::float8, 0.0) as avg_hours_to_core
             FROM lifecycle_transitions t
             JOIN atoms a ON t.atom_id = a.atom_id
             WHERE t.transitioned_at > $1
               AND t.to_lifecycle IN ('core', 'resolved')"
        )
        .bind(cutoff)
        .fetch_one(&self.pool)
        .await;

        match row {
            Ok(r) => {
                let transitions_in_window: i64 = r.get("transitions_in_window");
                let avg_hours_to_core: f64 = r.get("avg_hours_to_core");
                let rate_per_hour = if window_hours > 0.0 {
                    transitions_in_window as f64 / window_hours
                } else {
                    0.0
                };
                CrystallizationData { transitions_in_window, avg_hours_to_core, rate_per_hour }
            }
            Err(e) => {
                error!("crystallization_rate query failed: {}", e);
                CrystallizationData {
                    transitions_in_window: 0,
                    avg_hours_to_core: 0.0,
                    rate_per_hour: 0.0,
                }
            }
        }
    }

    /// Frontier diversity: Shannon entropy of the atom distribution across k embedding-space
    /// clusters.  High entropy means agents are covering the idea space broadly (good
    /// exploration).  Low entropy means herding around one cluster (bad).
    ///
    /// Pipeline: fetch active atom embeddings → random project 640→15 dims (fixed seed,
    /// stable across time) → k-means++ → Shannon entropy of cluster-size distribution.
    ///
    /// `k` is taken from the caller (sourced from `config.workers.frontier_diversity_k`).
    /// The `window` parameter scopes which atoms are included: only atoms created within
    /// the window AND whose embedding is ready are considered.  Pass `Duration::MAX` to
    /// include all atoms.
    pub async fn frontier_diversity(&self, window: Duration, k: usize) -> FrontierDiversityData {
        let cutoff: DateTime<Utc> = Utc::now() - chrono::Duration::from_std(window).unwrap_or_default();

        // Fetch 640-dim embeddings as raw float arrays from pgvector.
        // We cast the vector column to text and parse it because sqlx+pgvector returns
        // Vec<f32> via the pgvector crate's Vector type.
        let rows = sqlx::query(
            "SELECT embedding::text AS emb_text
             FROM atoms
             WHERE created_at > $1
               AND NOT archived
               AND NOT retracted
               AND embedding_status = 'ready'
               AND embedding IS NOT NULL"
        )
        .bind(cutoff)
        .fetch_all(&self.pool)
        .await;

        let embeddings: Vec<Vec<f32>> = match rows {
            Err(e) => {
                error!("frontier_diversity embedding query failed: {}", e);
                return FrontierDiversityData::default();
            }
            Ok(rows) => rows
                .iter()
                .filter_map(|r| {
                    let text: String = r.get("emb_text");
                    parse_pgvector_text(&text)
                })
                .collect(),
        };

        if embeddings.is_empty() {
            return FrontierDiversityData::default();
        }

        compute_frontier_diversity(&embeddings, k)
    }

    /// Contradiction resolution: how effectively the swarm converges on contested atoms.
    pub async fn contradiction_resolution(&self) -> ContradictionData {
        let resolved_row = sqlx::query(
            "SELECT
                COUNT(*) as resolved_count,
                COALESCE(AVG(
                    EXTRACT(EPOCH FROM (t.transitioned_at - a.created_at)) / 3600.0
                )::float8, 0.0) as avg_hours_to_resolve
             FROM lifecycle_transitions t
             JOIN atoms a ON t.atom_id = a.atom_id
             WHERE t.from_lifecycle = 'contested'
               AND t.to_lifecycle = 'resolved'"
        )
        .fetch_one(&self.pool)
        .await;

        let contested_count: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM atoms WHERE lifecycle = 'contested' AND NOT archived AND NOT retracted"
        )
        .fetch_one(&self.pool)
        .await
        .unwrap_or(0);

        match resolved_row {
            Ok(r) => {
                let resolved_count: i64 = r.get("resolved_count");
                let avg_hours_to_resolve: f64 = r.get("avg_hours_to_resolve");
                let total = resolved_count + contested_count;
                let resolution_rate = if total > 0 {
                    resolved_count as f64 / total as f64
                } else {
                    0.0
                };
                ContradictionData {
                    resolved_count,
                    contested_count,
                    avg_hours_to_resolve,
                    resolution_rate,
                }
            }
            Err(e) => {
                error!("contradiction_resolution query failed: {}", e);
                ContradictionData {
                    resolved_count: 0,
                    contested_count,
                    avg_hours_to_resolve: 0.0,
                    resolution_rate: 0.0,
                }
            }
        }
    }

    /// Pheromone landscape structure: whether the attraction/repulsion landscape
    /// has developed non-trivial variance (structure) vs a flat/random baseline.
    pub async fn landscape_structure(&self) -> LandscapeData {
        let row = sqlx::query(
            "SELECT
                COALESCE(VAR_POP(ph_attraction::float8), 0.0) as ph_attraction_variance,
                COALESCE(AVG(ph_attraction::float8), 0.0)     as ph_attraction_mean,
                COALESCE(VAR_POP(ph_repulsion::float8), 0.0)  as ph_repulsion_variance,
                COUNT(*) as atom_count
             FROM atoms
             WHERE NOT archived AND NOT retracted"
        )
        .fetch_one(&self.pool)
        .await;

        match row {
            Ok(r) => {
                let ph_attraction_variance: f64 = r.get("ph_attraction_variance");
                let ph_attraction_mean: f64 = r.get("ph_attraction_mean");
                let ph_repulsion_variance: f64 = r.get("ph_repulsion_variance");
                let atom_count: i64 = r.get("atom_count");
                let std_dev = ph_attraction_variance.sqrt();
                let coefficient_of_variation = if ph_attraction_mean > 0.0 {
                    std_dev / ph_attraction_mean
                } else {
                    0.0
                };
                LandscapeData {
                    ph_attraction_variance,
                    ph_attraction_mean,
                    ph_repulsion_variance,
                    coefficient_of_variation,
                    atom_count,
                }
            }
            Err(e) => {
                error!("landscape_structure query failed: {}", e);
                LandscapeData {
                    ph_attraction_variance: 0.0,
                    ph_attraction_mean: 0.0,
                    ph_repulsion_variance: 0.0,
                    coefficient_of_variation: 0.0,
                    atom_count: 0,
                }
            }
        }
    }

    /// Information propagation: how quickly one agent's discovery influences others.
    /// Measured as citation lag — the time between a parent atom being published and
    /// a child atom that references it (via replicates/extends edges) being published.
    pub async fn information_propagation(&self) -> PropagationData {
        let row = sqlx::query(
            "SELECT
                COUNT(*) as citation_count,
                COALESCE(
                    PERCENTILE_CONT(0.5) WITHIN GROUP (
                        ORDER BY EXTRACT(EPOCH FROM (child_a.created_at - parent_a.created_at)) / 3600.0
                    ),
                    0.0
                ) as median_citation_lag_hours
             FROM edges e
             JOIN atoms child_a  ON child_a.atom_id  = e.source_id
             JOIN atoms parent_a ON parent_a.atom_id = e.target_id
             WHERE e.type IN ('replicates', 'extends', 'derives')
               AND child_a.created_at > parent_a.created_at"
        )
        .fetch_one(&self.pool)
        .await;

        let agent_count: i64 = sqlx::query_scalar(
            "SELECT COUNT(DISTINCT author_agent_id) FROM atoms WHERE NOT archived AND NOT retracted"
        )
        .fetch_one(&self.pool)
        .await
        .unwrap_or(0);

        match row {
            Ok(r) => {
                let citation_count: i64 = r.get("citation_count");
                let median_citation_lag_hours: f64 = r.get("median_citation_lag_hours");
                PropagationData { median_citation_lag_hours, citation_count, agent_count }
            }
            Err(e) => {
                error!("information_propagation query failed: {}", e);
                PropagationData {
                    median_citation_lag_hours: 0.0,
                    citation_count: 0,
                    agent_count,
                }
            }
        }
    }

    /// Record a lifecycle transition for the audit trail used by crystallization
    /// and contradiction_resolution metrics. Called from LifecycleWorker.
    pub async fn record_transition(pool: &PgPool, atom_id: &str, from: &str, to: &str) {
        if let Err(e) = sqlx::query(
            "INSERT INTO lifecycle_transitions (atom_id, from_lifecycle, to_lifecycle)
             VALUES ($1, $2, $3)"
        )
        .bind(atom_id)
        .bind(from)
        .bind(to)
        .execute(pool)
        .await
        {
            error!("Failed to record lifecycle transition for {}: {}", atom_id, e);
        }
    }
}

// ─── Helpers ──────────────────────────────────────────────────────────────────

/// Parse a pgvector text representation like "[0.1,0.2,-0.3,...]" into Vec<f32>.
fn parse_pgvector_text(s: &str) -> Option<Vec<f32>> {
    let inner = s.trim().trim_start_matches('[').trim_end_matches(']');
    if inner.is_empty() {
        return None;
    }
    let values: Result<Vec<f32>, _> = inner
        .split(',')
        .map(|v| v.trim().parse::<f32>())
        .collect();
    values.ok().filter(|v| !v.is_empty())
}
