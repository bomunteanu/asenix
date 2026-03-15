use sqlx::PgPool;
use tracing::{info, error, debug};
use std::time::Duration;

/// System agent ID used by the bounty worker when publishing bounty atoms.
const SYSTEM_AGENT_ID: &str = "system-bounty-worker";

pub struct BountyWorker {
    pool: PgPool,
    novelty_threshold: f64,
    exploration_samples: u32,
    exploration_density_radius: f32,
    embedding_dimension: usize,
    sparse_region_max_atoms: i64,
}

impl BountyWorker {
    pub fn new(
        pool: PgPool,
        novelty_threshold: f64,
        exploration_samples: u32,
        exploration_density_radius: f32,
        embedding_dimension: usize,
        sparse_region_max_atoms: i64,
    ) -> Self {
        Self {
            pool,
            novelty_threshold,
            exploration_samples,
            exploration_density_radius,
            embedding_dimension,
            sparse_region_max_atoms,
        }
    }

    /// Ensure the system agent row exists so bounty atoms have a valid FK.
    async fn ensure_system_agent(&self) -> Result<(), sqlx::Error> {
        sqlx::query(
            "INSERT INTO agents (agent_id, public_key, confirmed, created_at) \
             VALUES ($1, decode('deadbeef', 'hex'), true, NOW()) \
             ON CONFLICT (agent_id) DO NOTHING",
        )
        .bind(SYSTEM_AGENT_ID)
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    /// One full tick: find high-novelty domains, locate sparse regions, publish bounties.
    pub async fn run_bounty_tick(
        &self,
    ) -> Result<usize, Box<dyn std::error::Error + Send + Sync>> {
        self.ensure_system_agent().await?;

        let domain_stats = crate::db::queries::get_domain_novelty_stats(&self.pool).await?;

        if domain_stats.is_empty() {
            debug!("Bounty worker: no domains with atoms found, skipping tick");
            return Ok(0);
        }

        let mut bounties_published = 0usize;
        let mut rng = {
            use rand::SeedableRng;
            rand::rngs::StdRng::from_os_rng()
        };

        for (domain, mean_novelty) in domain_stats {
            if mean_novelty <= self.novelty_threshold {
                continue;
            }

            info!(
                "High-novelty domain '{}': mean_novelty={:.3} > threshold={:.3}",
                domain, mean_novelty, self.novelty_threshold
            );

            // Sample random vectors to find a sparse region within this domain.
            let mut sparse_nearest_atom_id: Option<String> = None;

            for _ in 0..self.exploration_samples {
                use rand::Rng;

                let raw: Vec<f32> = (0..self.embedding_dimension)
                    .map(|_| rng.random::<f32>() * 2.0 - 1.0)
                    .collect();

                let norm: f32 = raw.iter().map(|x| x * x).sum::<f32>().sqrt();
                if norm == 0.0 {
                    continue;
                }
                let random_vector: Vec<f32> = raw.iter().map(|x| x / norm).collect();

                let (nearest_atom_opt, atom_count) =
                    crate::db::queries::query_nearest_atom_with_density(
                        &self.pool,
                        random_vector,
                        self.exploration_density_radius,
                    )
                    .await?;

                if atom_count < self.sparse_region_max_atoms {
                    if let Some(nearest) = nearest_atom_opt {
                        sparse_nearest_atom_id = Some(nearest.atom_id);
                        break;
                    }
                }
            }

            let nearest_atom_id = match sparse_nearest_atom_id {
                Some(id) => id,
                None => {
                    debug!(
                        "No sparse region found for domain '{}' after {} samples (all had >= {} nearby atoms)",
                        domain, self.exploration_samples, self.sparse_region_max_atoms
                    );
                    continue;
                }
            };

            // Publish the bounty atom via the internal path — not via MCP.
            let atom_input = crate::domain::atom::AtomInput {
                atom_type: crate::domain::atom::AtomType::Bounty,
                domain: domain.clone(),
                project_id: None,
                statement: format!(
                    "Sparse region near atom {}: explore this area to expand knowledge coverage in domain '{}'.",
                    nearest_atom_id, domain
                ),
                conditions: serde_json::json!({}),
                metrics: None,
                provenance: serde_json::json!({
                    "parent_ids": [],
                    "code_hash": "system-bounty-worker",
                    "environment": "system"
                }),
                signature: vec![0u8; 64],
                artifact_tree_hash: None,
                artifact_inline: None,
            };

            match crate::db::queries::publish_atom(&self.pool, SYSTEM_AGENT_ID, atom_input).await {
                Ok(bounty_atom_id) => {
                    info!(
                        "Published bounty atom '{}' for domain '{}' (nearest atom: '{}')",
                        bounty_atom_id, domain, nearest_atom_id
                    );

                    // Add inspired_by edge from the new bounty atom to the nearest atom.
                    if let Err(e) = sqlx::query(
                        "INSERT INTO edges (source_id, target_id, type) \
                         VALUES ($1, $2, 'inspired_by') \
                         ON CONFLICT (source_id, target_id, type) DO NOTHING",
                    )
                    .bind(&bounty_atom_id)
                    .bind(&nearest_atom_id)
                    .execute(&self.pool)
                    .await
                    {
                        error!("Failed to insert inspired_by edge for bounty '{}': {}", bounty_atom_id, e);
                    }

                    bounties_published += 1;
                }
                Err(e) => {
                    error!(
                        "Failed to publish bounty atom for domain '{}': {}",
                        domain, e
                    );
                }
            }
        }

        Ok(bounties_published)
    }

    /// Start the periodic bounty worker loop.
    pub async fn start(self, interval_minutes: u64) {
        let mut interval =
            tokio::time::interval(Duration::from_secs(interval_minutes * 60));

        loop {
            interval.tick().await;
            match self.run_bounty_tick().await {
                Ok(count) => {
                    if count > 0 {
                        info!("Bounty worker published {} bounty atom(s) this tick", count);
                    }
                }
                Err(e) => {
                    error!("Bounty worker tick failed: {}", e);
                }
            }
        }
    }
}
