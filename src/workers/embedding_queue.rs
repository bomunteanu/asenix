use crate::config::Config;
use crate::domain::atom::AtomType;
use crate::domain::pheromone::*;
use crate::domain::condition::ConditionRegistry;
use crate::db::graph_cache::{GraphCache, EdgeType};
use crate::embedding::provider::EmbeddingProvider;
use crate::error::{MoteError, Result};
use pgvector::Vector;
use sqlx::{PgPool, Row};
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::RwLock;
use tracing::{info, error, debug, warn};
use serde_json::Value;

pub struct EmbeddingQueue {
    pool: PgPool,
    config: Config,
    graph_cache: Arc<RwLock<GraphCache>>,
    condition_registry: Arc<RwLock<ConditionRegistry>>,
    provider: Arc<EmbeddingProvider>,
}

impl EmbeddingQueue {
    pub fn new(
        pool: PgPool,
        config: Config,
        graph_cache: Arc<RwLock<GraphCache>>,
        condition_registry: Arc<RwLock<ConditionRegistry>>,
        provider: Arc<EmbeddingProvider>,
    ) -> Self {
        Self {
            pool,
            config,
            graph_cache,
            condition_registry,
            provider,
        }
    }

    /// Process atoms with pending embeddings
    pub async fn process_pending(&self) -> Result<usize> {
        let atoms = sqlx::query(
            "SELECT atom_id FROM atoms
             WHERE embedding_status = 'pending' AND NOT archived
             ORDER BY created_at ASC
             LIMIT 100"
        )
        .fetch_all(&self.pool)
        .await?;

        let mut processed = 0;
        for row in atoms {
            let atom_id: String = row.get("atom_id");
            match self.process_atom(&atom_id).await {
                Ok(_) => {
                    processed += 1;
                    info!("Processed embedding for atom: {}", atom_id);
                }
                Err(e) => {
                    error!("Failed to process embedding for atom {}: {}", atom_id, e);
                }
            }
        }
        Ok(processed)
    }

    /// Start the embedding worker
    pub async fn start(&self) {
        info!("Starting embedding worker");
        let mut interval = tokio::time::interval(Duration::from_secs(30));
        loop {
            tokio::select! {
                _ = interval.tick() => {
                    match self.process_pending().await {
                        Ok(count) => { if count > 0 { info!("Processed {} embeddings", count); } }
                        Err(e) => { error!("Error processing embeddings: {}", e); }
                    }
                }
            }
        }
    }

    async fn process_atom(&self, atom_id: &str) -> Result<()> {
        let embedding = self.generate_embedding(atom_id).await?;
        let pg_vector = Vector::from(embedding.clone());
        sqlx::query("UPDATE atoms SET embedding = $1, embedding_status = 'ready' WHERE atom_id = $2")
            .bind(&pg_vector)
            .bind(atom_id)
            .execute(&self.pool)
            .await?;
        self.update_pheromone_neighbourhood(atom_id, &embedding).await?;
        self.generate_summary(atom_id).await?;
        Ok(())
    }

    async fn generate_summary(&self, atom_id: &str) -> Result<()> {
        let (endpoint, model) = match (&self.config.hub.summary_llm_endpoint, &self.config.hub.summary_llm_model) {
            (Some(endpoint), Some(model)) => (endpoint, model),
            _ => {
                debug!("Summary generation not configured, skipping for atom {}", atom_id);
                return Ok(());
            }
        };

        let atom_row = sqlx::query(
            "SELECT type, domain, statement, conditions, metrics FROM atoms WHERE atom_id = $1"
        )
        .bind(atom_id)
        .fetch_optional(&self.pool)
        .await?
        .ok_or_else(|| MoteError::NotFound("Atom not found".to_string()))?;

        let atom_type: String = atom_row.get("type");
        let domain: String = atom_row.get("domain");
        let statement: String = atom_row.get("statement");
        let conditions: Value = atom_row.get("conditions");
        let metrics: Option<Value> = atom_row.get("metrics");

        let client = reqwest::Client::new();
        let request_body = serde_json::json!({
            "model": model,
            "messages": [
                {"role": "system", "content": "Summarize the following research atom in one paragraph for a human reader."},
                {"role": "user", "content": format!(
                    "Type: {}\nDomain: {}\nStatement: {}\nConditions: {}\nMetrics: {}",
                    atom_type, domain, statement,
                    serde_json::to_string_pretty(&conditions).unwrap_or_default(),
                    metrics.map(|m| serde_json::to_string_pretty(&m).unwrap_or_default()).unwrap_or_else(|| "None".to_string())
                )}
            ],
            "max_tokens": 300,
            "temperature": 0.3
        });

        match tokio::time::timeout(Duration::from_secs(30), client.post(endpoint).json(&request_body).send()).await {
            Ok(Ok(response)) if response.status().is_success() => {
                match response.json::<serde_json::Value>().await {
                    Ok(j) => {
                        if let Some(summary) = j.get("choices").and_then(|c| c.get(0))
                            .and_then(|c| c.get("message")).and_then(|m| m.get("content"))
                            .and_then(|c| c.as_str())
                        {
                            sqlx::query("UPDATE atoms SET summary = $1 WHERE atom_id = $2")
                                .bind(summary.trim()).bind(atom_id)
                                .execute(&self.pool).await?;
                            info!("Generated summary for atom {}", atom_id);
                        }
                    }
                    Err(e) => warn!("Failed to parse LLM response for atom {}: {}", atom_id, e),
                }
            }
            Ok(Ok(r)) => warn!("LLM request failed for atom {}: {}", atom_id, r.status()),
            Ok(Err(e)) => warn!("Failed to call LLM for atom {}: {}", atom_id, e),
            Err(_) => warn!("LLM request timeout for atom {}", atom_id),
        }
        Ok(())
    }

    async fn generate_embedding(&self, atom_id: &str) -> Result<Vec<f32>> {
        let statement: String = sqlx::query_scalar("SELECT statement FROM atoms WHERE atom_id = $1")
            .bind(atom_id)
            .fetch_one(&self.pool)
            .await?;
        let embedding = self.provider.encode(&statement).await?;
        let expected = self.config.hub.embedding_dimension;
        if embedding.len() != expected {
            return Err(MoteError::Internal(format!(
                "Embedding dimension mismatch: provider returned {} but config expects {}",
                embedding.len(), expected
            )));
        }
        Ok(embedding)
    }

    /// Update pheromone values for the neighbourhood of a newly embedded atom.
    async fn update_pheromone_neighbourhood(&self, atom_id: &str, embedding: &[f32]) -> Result<()> {
        let atom_row = sqlx::query(
            "SELECT type, domain, conditions, metrics FROM atoms WHERE atom_id = $1"
        )
        .bind(atom_id)
        .fetch_optional(&self.pool)
        .await?
        .ok_or_else(|| MoteError::NotFound("Atom not found".to_string()))?;

        let atom_type_str: String = atom_row.get("type");
        let atom_type = match atom_type_str.as_str() {
            "finding"         => AtomType::Finding,
            "negative_result" => AtomType::NegativeResult,
            "hypothesis"      => AtomType::Hypothesis,
            "bounty"          => AtomType::Bounty,
            "synthesis"       => AtomType::Synthesis,
            _ => {
                debug!("Atom type {} doesn't trigger pheromone updates", atom_type_str);
                return Ok(());
            }
        };

        let domain: String   = atom_row.get("domain");
        let conditions: Value = atom_row.get("conditions");
        let metrics: Option<Value> = atom_row.get("metrics");

        let neighbours = self.find_neighbours(embedding, &domain).await?;
        let mut updates: Vec<(String, &str, f64)> = Vec::new();

        // ── Fix 4: per-atom novelty (not a shared count) ─────────────────────
        // New atom's novelty = 1 / (1 + its_neighbourhood_size)
        updates.push((atom_id.to_string(), "ph_novelty", novelty(neighbours.len())));

        // For each existing neighbour: they now have one more neighbour (the new atom).
        // Derive their new local count from their stored novelty: count = round(1/novelty - 1).
        for nbr in &neighbours {
            let old_count = if nbr.ph_novelty > 0.0 {
                (1.0 / nbr.ph_novelty - 1.0).round() as usize
            } else {
                neighbours.len()
            };
            updates.push((nbr.atom_id.clone(), "ph_novelty", novelty(old_count + 1)));
        }

        // ── Attraction / repulsion by atom type ───────────────────────────────
        match atom_type {
            AtomType::Finding => {
                if let Some(ref m) = metrics {
                    for metric_name in array_metric_names(m) {
                        let new_val = match extract_array_metric_value(m, &metric_name) {
                            Some(v) => v,
                            None => continue,
                        };
                        let higher_better = is_array_metric_higher_better(m, &metric_name);
                        // Best existing value for this metric among neighbours
                        let best: Option<f64> = neighbours.iter()
                            .filter_map(|n| n.metrics.as_ref())
                            .filter_map(|nm| extract_array_metric_value(nm, &metric_name))
                            .reduce(|acc, v| {
                                if higher_better { f64::max(acc, v) } else { f64::min(acc, v) }
                            });
                        let boost = attraction_boost(new_val, best, self.config.pheromone.attraction_cap, 1.0);
                        if boost > 0.0 {
                            for nbr in &neighbours {
                                updates.push((nbr.atom_id.clone(), "ph_attraction",
                                    nbr.ph_attraction + boost));
                            }
                        }
                    }
                }
                // New atom's own attraction = neighbourhood average
                let avg = if neighbours.is_empty() { 0.0 }
                    else { neighbours.iter().map(|n| n.ph_attraction).sum::<f64>() / neighbours.len() as f64 };
                updates.push((atom_id.to_string(), "ph_attraction", avg));
            }

            AtomType::NegativeResult => {
                // Fix 2: repulsion belongs to the negative-result atom only,
                // not spread to neighbours who had nothing to do with the failure.
                updates.push((atom_id.to_string(), "ph_repulsion", repulsion_increment()));
            }

            _ => {} // Hypotheses, bounties, etc. get only novelty updates
        }

        // ── Contradiction detection ───────────────────────────────────────────
        let contradictions = self.detect_contradictions(
            atom_id, &atom_type, &domain, &conditions, &metrics, &neighbours,
        ).await?;

        for c in &contradictions {
            updates.push((c.atom_id.clone(), "ph_disagreement", c.new_disagreement));
        }

        self.apply_pheromone_updates(updates).await?;

        for c in contradictions {
            self.insert_contradicts_edge(&c.atom_id, atom_id).await?;
        }

        // ── Fix 3: Replication detection ──────────────────────────────────────
        if matches!(atom_type, AtomType::Finding) {
            let replications = self.detect_replications(
                atom_id, &conditions, &metrics, &neighbours,
            ).await?;
            for r in replications {
                self.record_replication(atom_id, &r.atom_id).await?;
            }
        }

        info!("Updated pheromone for {} neighbours of atom {}", neighbours.len(), atom_id);
        Ok(())
    }

    /// Find atoms within neighbourhood radius of the given embedding.
    async fn find_neighbours(&self, embedding: &[f32], domain: &str) -> Result<Vec<NeighbourInfo>> {
        let pg_vector = Vector::from(embedding.to_vec());
        let rows = sqlx::query(
            "SELECT atom_id, ph_attraction, ph_repulsion, ph_novelty, conditions, metrics
             FROM atoms
             WHERE embedding IS NOT NULL
               AND embedding_status = 'ready'
               AND domain = $1
               AND NOT archived
               AND embedding <=> $2 < $3"
        )
        .bind(domain)
        .bind(&pg_vector)
        .bind(self.config.hub.neighbourhood_radius)
        .fetch_all(&self.pool)
        .await?;

        let mut neighbours = Vec::new();
        for row in rows {
            let ph_attraction: f32 = row.get("ph_attraction");
            let ph_repulsion: f32  = row.get("ph_repulsion");
            let ph_novelty: f32    = row.get("ph_novelty");
            neighbours.push(NeighbourInfo {
                atom_id:      row.get("atom_id"),
                ph_attraction: ph_attraction as f64,
                ph_repulsion:  ph_repulsion as f64,
                ph_novelty:    ph_novelty as f64,
                conditions:    row.get("conditions"),
                metrics:       row.get("metrics"),
            });
        }
        Ok(neighbours)
    }

    /// Detect contradictions between the new atom and neighbours.
    async fn detect_contradictions(
        &self,
        new_atom_id: &str,
        new_atom_type: &AtomType,
        _domain: &str,
        conditions: &Value,
        metrics: &Option<Value>,
        neighbours: &[NeighbourInfo],
    ) -> Result<Vec<ContradictionInfo>> {
        let mut contradictions = Vec::new();
        if !matches!(new_atom_type, AtomType::Finding) {
            return Ok(contradictions);
        }
        let Some(metrics) = metrics.as_ref() else { return Ok(contradictions) };

        for nbr in neighbours {
            let Some(ref nbr_metrics) = nbr.metrics else { continue };
            // Fix 3: compare conditions to conditions (was incorrectly comparing conditions to metrics)
            if !conditions_shared_keys_equivalent(conditions, &nbr.conditions) {
                continue;
            }
            if let Some(true) = self.check_metric_contradiction(metrics, nbr_metrics).await? {
                if !self.contradicts_edge_exists(new_atom_id, &nbr.atom_id).await? {
                    contradictions.push(ContradictionInfo {
                        atom_id: nbr.atom_id.clone(),
                        new_disagreement: 0.0,
                    });
                }
            }
        }

        for c in &mut contradictions {
            let (contradicts_edges, total_edges) = self.count_edge_types(&c.atom_id).await?;
            c.new_disagreement = disagreement(contradicts_edges, total_edges);
        }
        Ok(contradictions)
    }

    /// Detect replications: same conditions, agreeing metrics.
    async fn detect_replications(
        &self,
        new_atom_id: &str,
        conditions: &Value,
        metrics: &Option<Value>,
        neighbours: &[NeighbourInfo],
    ) -> Result<Vec<ReplicationInfo>> {
        let Some(metrics) = metrics.as_ref() else { return Ok(vec![]) };
        let mut replications = Vec::new();

        for nbr in neighbours {
            let Some(ref nbr_metrics) = nbr.metrics else { continue };
            if !conditions_shared_keys_equivalent(conditions, &nbr.conditions) {
                continue;
            }
            // Metrics agree = same direction, no dramatic contradiction
            if !self.metrics_agree(metrics, nbr_metrics) {
                continue;
            }
            if !self.replicates_edge_exists(new_atom_id, &nbr.atom_id).await? {
                replications.push(ReplicationInfo { atom_id: nbr.atom_id.clone() });
            }
        }
        Ok(replications)
    }

    /// Insert replicates edge and advance lifecycle of the replicated atom.
    async fn record_replication(&self, new_atom_id: &str, existing_atom_id: &str) -> Result<()> {
        sqlx::query(
            "INSERT INTO edges (source_id, target_id, type) VALUES ($1, $2, 'replicates')
             ON CONFLICT (source_id, target_id, type) DO NOTHING"
        )
        .bind(new_atom_id)
        .bind(existing_atom_id)
        .execute(&self.pool)
        .await?;

        // Bump repl_exact counter on the replicated atom
        sqlx::query(
            "UPDATE atoms SET repl_exact = repl_exact + 1 WHERE atom_id = $1"
        )
        .bind(existing_atom_id)
        .execute(&self.pool)
        .await?;

        // Advance lifecycle: provisional → replicated when repl_exact >= 1
        sqlx::query(
            "UPDATE atoms SET lifecycle = 'replicated'
             WHERE atom_id = $1 AND lifecycle = 'provisional' AND repl_exact >= 1"
        )
        .bind(existing_atom_id)
        .execute(&self.pool)
        .await?;

        // Update graph cache
        let mut cache = self.graph_cache.write().await;
        cache.add_edge(new_atom_id, existing_atom_id, EdgeType::Replicates)?;

        info!("Recorded replication: {} replicates {}", new_atom_id, existing_atom_id);
        Ok(())
    }

    /// True if all shared keys in both condition objects have equal values.
    /// Returns false if there are no shared keys (atoms not comparable).
    fn check_metric_contradiction<'a>(
        &'a self,
        metrics1: &'a Value,
        metrics2: &'a Value,
    ) -> std::pin::Pin<Box<dyn std::future::Future<Output = Result<Option<bool>>> + Send + 'a>> {
        Box::pin(async move {
            // metrics are arrays: [{name, value, direction}, ...]
            let names1 = array_metric_names(metrics1);
            for name in &names1 {
                let Some(v1) = extract_array_metric_value(metrics1, name) else { continue };
                let Some(v2) = extract_array_metric_value(metrics2, name) else { continue };
                let higher_better = is_array_metric_higher_better(metrics1, name);
                if metrics_contradict(v1, v2, higher_better, 0.1) {
                    return Ok(Some(true));
                }
            }
            Ok(Some(false))
        })
    }

    /// Returns true if metrics from two atoms agree (same direction, not contradicting).
    fn metrics_agree(&self, metrics1: &Value, metrics2: &Value) -> bool {
        let names = array_metric_names(metrics1);
        if names.is_empty() { return false; }
        let mut any_shared = false;
        for name in &names {
            let Some(v1) = extract_array_metric_value(metrics1, name) else { continue };
            let Some(v2) = extract_array_metric_value(metrics2, name) else { continue };
            any_shared = true;
            let higher_better = is_array_metric_higher_better(metrics1, name);
            if metrics_contradict(v1, v2, higher_better, 0.15) {
                return false; // significant disagreement → not a replication
            }
        }
        any_shared
    }

    async fn contradicts_edge_exists(&self, atom1_id: &str, atom2_id: &str) -> Result<bool> {
        let count: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM edges
             WHERE ((source_id = $1 AND target_id = $2) OR (source_id = $2 AND target_id = $1))
             AND type = 'contradicts'"
        )
        .bind(atom1_id).bind(atom2_id)
        .fetch_one(&self.pool)
        .await?;
        Ok(count > 0)
    }

    async fn replicates_edge_exists(&self, atom1_id: &str, atom2_id: &str) -> Result<bool> {
        let count: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM edges
             WHERE source_id = $1 AND target_id = $2 AND type = 'replicates'"
        )
        .bind(atom1_id).bind(atom2_id)
        .fetch_one(&self.pool)
        .await?;
        Ok(count > 0)
    }

    async fn count_edge_types(&self, atom_id: &str) -> Result<(usize, usize)> {
        let row = sqlx::query(
            "SELECT
                COUNT(*) FILTER (WHERE type = 'contradicts') as contradicts_count,
                COUNT(*) as total_count
             FROM edges WHERE source_id = $1 OR target_id = $1"
        )
        .bind(atom_id)
        .fetch_one(&self.pool)
        .await?;
        let contradicts: i64 = row.get("contradicts_count");
        let total: i64       = row.get("total_count");
        Ok((contradicts as usize, total as usize))
    }

    async fn insert_contradicts_edge(&self, atom1_id: &str, atom2_id: &str) -> Result<()> {
        sqlx::query(
            "INSERT INTO edges (source_id, target_id, type) VALUES
             ($1, $2, 'contradicts'), ($2, $1, 'contradicts')
             ON CONFLICT (source_id, target_id, type) DO NOTHING"
        )
        .bind(atom1_id).bind(atom2_id)
        .execute(&self.pool)
        .await?;

        let mut cache = self.graph_cache.write().await;
        cache.add_edge(atom1_id, atom2_id, EdgeType::Contradicts)?;
        cache.add_edge(atom2_id, atom1_id, EdgeType::Contradicts)?;
        Ok(())
    }

    async fn apply_pheromone_updates(&self, updates: Vec<(String, &str, f64)>) -> Result<()> {
        if updates.is_empty() { return Ok(()); }

        // Merge updates per (atom_id, field) — last write wins
        let mut merged: std::collections::HashMap<(String, String), f64> = std::collections::HashMap::new();
        for (atom_id, field, value) in updates {
            merged.insert((atom_id, field.to_string()), value);
        }

        for ((atom_id, field), value) in merged {
            sqlx::query(&format!("UPDATE atoms SET {} = $1 WHERE atom_id = $2", field))
                .bind(value as f32)
                .bind(&atom_id)
                .execute(&self.pool)
                .await?;
        }
        Ok(())
    }
}

/// Shared-key equivalence: conditions match when all shared keys have equal values
/// and there is at least one shared key.
fn conditions_shared_keys_equivalent(c1: &Value, c2: &Value) -> bool {
    match (c1.as_object(), c2.as_object()) {
        (Some(m1), Some(m2)) => {
            let shared: Vec<_> = m1.keys().filter(|k| m2.contains_key(*k)).collect();
            !shared.is_empty() && shared.iter().all(|k| m1.get(*k) == m2.get(*k))
        }
        _ => false,
    }
}

#[derive(Debug, Clone)]
struct NeighbourInfo {
    atom_id:      String,
    ph_attraction: f64,
    ph_repulsion:  f64,
    ph_novelty:    f64,
    conditions:    Value,
    metrics:       Option<Value>,
}

#[derive(Debug, Clone)]
struct ContradictionInfo {
    atom_id:          String,
    new_disagreement: f64,
}

#[derive(Debug, Clone)]
struct ReplicationInfo {
    atom_id: String,
}
