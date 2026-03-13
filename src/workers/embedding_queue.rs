use crate::config::Config;
use crate::domain::atom::AtomType;
use crate::domain::pheromone::*;
use crate::domain::condition::ConditionRegistry;
use crate::db::graph_cache::{GraphCache, EdgeType};
use crate::error::{MoteError, Result};
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
}

impl EmbeddingQueue {
    pub fn new(
        pool: PgPool,
        config: Config,
        graph_cache: Arc<RwLock<GraphCache>>,
        condition_registry: Arc<RwLock<ConditionRegistry>>,
    ) -> Self {
        Self {
            pool,
            config,
            graph_cache,
            condition_registry,
        }
    }

    /// Process atoms with pending embeddings
    pub async fn process_pending(&self) -> Result<usize> {
        // Get atoms with pending embeddings
        let atoms = sqlx::query(
            "SELECT atom_id, type, domain, statement, conditions, metrics, provenance, 
                    author_agent_id, created_at, signature, confidence, ph_attraction, ph_repulsion, 
                    ph_novelty, ph_disagreement, embedding, embedding_status, repl_exact, repl_conceptual, 
                    repl_extension, traffic, lifecycle, retracted, retraction_reason, ban_flag, 
                    archived, probationary, summary 
             FROM atoms 
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
                    info!("Successfully processed embedding for atom: {}", atom_id);
                }
                Err(e) => {
                    error!("Failed to process embedding for atom {}: {}", atom_id, e);
                    // Mark as permanently failed after multiple attempts would go here
                    // For MVP, we just continue with next atom
                }
            }
        }

        Ok(processed)
    }

    /// Start the embedding worker - continuously processes pending embeddings
    pub async fn start(&self) {
        info!("Starting embedding worker");
        let mut interval = tokio::time::interval(Duration::from_secs(30)); // Check every 30 seconds
        
        loop {
            tokio::select! {
                _ = interval.tick() => {
                    match self.process_pending().await {
                        Ok(count) => {
                            if count > 0 {
                                info!("Processed {} embeddings", count);
                            }
                        }
                        Err(e) => {
                            error!("Error processing embeddings: {}", e);
                        }
                    }
                }
            }
        }
    }

    /// Process a single atom's embedding and pheromone update
    async fn process_atom(&self, atom_id: &str) -> Result<()> {
        // Step 1: Generate embedding (placeholder - would call actual embedding service)
        let embedding = self.generate_embedding(atom_id).await?;
        
        // Step 2: Update atom with embedding
        sqlx::query(
            "UPDATE atoms SET embedding = $1, embedding_status = 'ready' WHERE atom_id = $2"
        )
        .bind(&embedding)
        .bind(atom_id)
        .execute(&self.pool)
        .await?;

        // Step 3: Perform pheromone neighbourhood update
        self.update_pheromone_neighbourhood(atom_id, &embedding).await?;
        
        // Step 4: Generate summary (optional)
        self.generate_summary(atom_id).await?;
        
        Ok(())
    }

    /// Generate summary for an atom using configured LLM endpoint
    async fn generate_summary(&self, atom_id: &str) -> Result<()> {
        // Check if summary generation is configured
        let (endpoint, model) = match (&self.config.hub.summary_llm_endpoint, &self.config.hub.summary_llm_model) {
            (Some(endpoint), Some(model)) => (endpoint, model),
            _ => {
                debug!("Summary generation not configured, skipping for atom {}", atom_id);
                return Ok(());
            }
        };

        // Load atom data for summary generation
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

        // Prepare LLM request
        let client = reqwest::Client::new();
        let request_body = serde_json::json!({
            "model": model,
            "messages": [
                {
                    "role": "system",
                    "content": "Summarize the following research atom in one paragraph for a human reader. The summary should be self-contained and capture the key contribution without requiring the reader to see the structured fields."
                },
                {
                    "role": "user",
                    "content": format!(
                        "Type: {}\nDomain: {}\nStatement: {}\nConditions: {}\nMetrics: {}",
                        atom_type,
                        domain,
                        statement,
                        serde_json::to_string_pretty(&conditions).unwrap_or_default(),
                        metrics.map(|m| serde_json::to_string_pretty(&m).unwrap_or_default()).unwrap_or_else(|| "None".to_string())
                    )
                }
            ],
            "max_tokens": 300,
            "temperature": 0.3
        });

        // Call LLM with timeout
        match tokio::time::timeout(
            Duration::from_secs(30),
            client
                .post(endpoint)
                .json(&request_body)
                .send()
        ).await {
            Ok(Ok(response)) => {
                if response.status().is_success() {
                    match response.json::<serde_json::Value>().await {
                        Ok(response_json) => {
                            if let Some(summary) = response_json
                                .get("choices")
                                .and_then(|c| c.get(0))
                                .and_then(|c| c.get("message"))
                                .and_then(|m| m.get("content"))
                                .and_then(|c| c.as_str())
                            {
                                // Update atom with summary
                                sqlx::query("UPDATE atoms SET summary = $1 WHERE atom_id = $2")
                                    .bind(summary.trim())
                                    .bind(atom_id)
                                    .execute(&self.pool)
                                    .await?;
                                
                                info!("Generated summary for atom {}", atom_id);
                            } else {
                                warn!("Invalid LLM response format for atom {}", atom_id);
                            }
                        }
                        Err(e) => {
                            error!("Failed to parse LLM response for atom {}: {}", atom_id, e);
                        }
                    }
                } else {
                    error!("LLM request failed for atom {}: {}", atom_id, response.status());
                }
            }
            Ok(Err(e)) => {
                error!("Failed to call LLM for atom {}: {}", atom_id, e);
            }
            Err(_) => {
                error!("LLM request timeout for atom {}", atom_id);
            }
        }

        Ok(())
    }
    async fn generate_embedding(&self, atom_id: &str) -> Result<Vec<f64>> {
        // This would call the actual embedding service
        // For MVP, generate a simple deterministic embedding based on atom_id
        let dimension = self.config.hub.embedding_dimension;
        let mut embedding = vec![0.0; dimension];
        
        // Simple hash-based embedding for testing
        use std::collections::hash_map::DefaultHasher;
        use std::hash::{Hash, Hasher};
        
        let mut hasher = DefaultHasher::new();
        atom_id.hash(&mut hasher);
        let hash = hasher.finish();
        
        for (i, val) in embedding.iter_mut().enumerate() {
            *val = ((hash >> (i % 8)) as f64 / u64::MAX as f64) * 2.0 - 1.0;
        }
        
        Ok(embedding)
    }

    /// Update pheromone values for the neighbourhood of a newly embedded atom
    async fn update_pheromone_neighbourhood(&self, atom_id: &str, embedding: &[f64]) -> Result<()> {
        // Load the new atom's data
        let atom_row = sqlx::query(
            "SELECT type, domain, conditions, metrics, ph_attraction, ph_repulsion, ph_novelty, ph_disagreement
             FROM atoms WHERE atom_id = $1"
        )
        .bind(atom_id)
        .fetch_optional(&self.pool)
        .await?
        .ok_or_else(|| MoteError::NotFound("Atom not found".to_string()))?;

        let atom_type_str: String = atom_row.get("type");
        let atom_type = match atom_type_str.as_str() {
            "finding" => AtomType::Finding,
            "negative_result" => AtomType::NegativeResult,
            "hypothesis" => AtomType::Hypothesis,
            "bounty" => AtomType::Bounty,
            "synthesis" => AtomType::Synthesis,
            _ => {
                debug!("Atom type {} doesn't trigger pheromone updates", atom_type_str);
                return Ok(());
            }
        };

        let domain: String = atom_row.get("domain");
        let conditions: Value = atom_row.get("conditions");
        let metrics: Option<Value> = atom_row.get("metrics");

        // Find neighbours within radius
        let neighbours = self.find_neighbours(embedding, &domain).await?;
        
        // Calculate new count for novelty (neighbours + 1 for new atom)
        let total_count = neighbours.len() + 1;
        let new_novelty = novelty(total_count);

        // Collect all pheromone updates
        let mut updates = Vec::new();
        
        // Update novelty for all atoms in neighbourhood (including new atom)
        updates.push((atom_id.to_string(), "ph_novelty", new_novelty));
        
        for neighbour in &neighbours {
            updates.push((neighbour.atom_id.clone(), "ph_novelty", new_novelty));
        }

        // Handle attraction/repulsion based on atom type
        match atom_type {
            AtomType::Finding => {
                if let Some(metrics) = &metrics {
                    // Find best metric values in neighbourhood for attraction boost
                    let metric_improvements = self.find_metric_improvements(metrics, &neighbours).await?;
                    
                    for (_metric_name, improvement) in metric_improvements {
                        if improvement > 0.0 {
                            // Add boost to all neighbours
                            for neighbour in &neighbours {
                                updates.push((neighbour.atom_id.clone(), "ph_attraction", 
                                    neighbour.ph_attraction + improvement));
                            }
                        }
                    }
                    
                    // Set new atom's attraction to neighbourhood average
                    let avg_attraction = if neighbours.is_empty() {
                        0.0
                    } else {
                        neighbours.iter().map(|n| n.ph_attraction).sum::<f64>() / neighbours.len() as f64
                    };
                    updates.push((atom_id.to_string(), "ph_attraction", avg_attraction));
                }
            }
            AtomType::NegativeResult => {
                // Add repulsion increment to all neighbours
                let repulsion_inc = repulsion_increment();
                for neighbour in &neighbours {
                    updates.push((neighbour.atom_id.clone(), "ph_repulsion", 
                        neighbour.ph_repulsion + repulsion_inc));
                }
                
                // Set new atom's repulsion to neighbourhood average + increment
                let avg_repulsion = if neighbours.is_empty() {
                    repulsion_inc
                } else {
                    neighbours.iter().map(|n| n.ph_repulsion).sum::<f64>() / neighbours.len() as f64 + repulsion_inc
                };
                updates.push((atom_id.to_string(), "ph_repulsion", avg_repulsion));
            }
            _ => {
                // Other atom types only get novelty updates
            }
        }

        // Perform contradiction detection
        let contradictions = self.detect_contradictions(
            atom_id, 
            &atom_type, 
            &domain, 
            &conditions, 
            &metrics, 
            &neighbours
        ).await?;

        // Update disagreement for atoms with new contradictions
        for contradiction in &contradictions {
            updates.push((contradiction.atom_id.clone(), "ph_disagreement", contradiction.new_disagreement));
        }

        // Apply all updates in a single batch
        self.apply_pheromone_updates(updates).await?;

        // Insert contradicts edges into database and graph cache
        for contradiction in contradictions {
            self.insert_contradicts_edge(&contradiction.atom_id, atom_id).await?;
        }

        info!("Updated pheromone for {} neighbours of atom {}", neighbours.len(), atom_id);
        Ok(())
    }

    /// Find atoms within neighbourhood radius of the given embedding
    async fn find_neighbours(&self, embedding: &[f64], domain: &str) -> Result<Vec<NeighbourInfo>> {
        let rows = sqlx::query(
            "SELECT atom_id, ph_attraction, ph_repulsion, metrics
             FROM atoms 
             WHERE embedding IS NOT NULL 
             AND embedding_status = 'ready' 
             AND domain = $1
             AND NOT archived
             AND embedding <=> $2 < $3"
        )
        .bind(domain)
        .bind(embedding)
        .bind(self.config.hub.neighbourhood_radius)
        .fetch_all(&self.pool)
        .await?;

        let mut neighbours = Vec::new();
        for row in rows {
            neighbours.push(NeighbourInfo {
                atom_id: row.get("atom_id"),
                ph_attraction: row.get("ph_attraction"),
                ph_repulsion: row.get("ph_repulsion"),
                metrics: row.get("metrics"),
            });
        }

        Ok(neighbours)
    }

    /// Find metric improvements in the neighbourhood for attraction calculation
    async fn find_metric_improvements(&self, metrics: &Value, neighbours: &[NeighbourInfo]) -> Result<Vec<(String, f64)>> {
        let mut improvements = Vec::new();
        
        if let Some(metrics_obj) = metrics.as_object() {
            for (metric_name, new_value) in metrics_obj {
                if let Some(new_value_f64) = new_value.as_f64() {
                    // Find best existing value in neighbourhood
                    let mut best_value: Option<f64> = None;
                    
                    for neighbour in neighbours {
                        if let Some(neighbour_metrics) = &neighbour.metrics {
                            if let Some(neighbour_value) = extract_metric_value(neighbour_metrics, metric_name) {
                                match best_value {
                                    None => best_value = Some(neighbour_value),
                                    Some(current_best) => {
                                        let higher_better = is_higher_better(metrics, metric_name);
                                        if (higher_better && neighbour_value > current_best)
                                            || (!higher_better && neighbour_value < current_best)
                                        {
                                            best_value = Some(neighbour_value);
                                        }
                                    }
                                }
                            }
                        }
                    }
                    
                    // Calculate attraction boost
                    let boost = attraction_boost(
                        new_value_f64,
                        best_value,
                        self.config.pheromone.attraction_cap,
                        1.0, // baseline boost
                    );
                    
                    if boost > 0.0 {
                        improvements.push((metric_name.clone(), boost));
                    }
                }
            }
        }

        Ok(improvements)
    }

    /// Detect contradictions between the new atom and neighbours
    async fn detect_contradictions(
        &self,
        new_atom_id: &str,
        new_atom_type: &AtomType,
        domain: &str,
        conditions: &Value,
        metrics: &Option<Value>,
        neighbours: &[NeighbourInfo],
    ) -> Result<Vec<ContradictionInfo>> {
        let mut contradictions = Vec::new();
        
        // Only findings can contradict
        if !matches!(new_atom_type, AtomType::Finding) || metrics.is_none() {
            return Ok(contradictions);
        }

        let metrics = metrics.as_ref().unwrap();
        let condition_registry = self.condition_registry.read().await;
        
        for neighbour in neighbours {
            // Skip if neighbour has no metrics
            if neighbour.metrics.is_none() {
                continue;
            }
            
            let neighbour_metrics = neighbour.metrics.as_ref().unwrap();
            
            // Check condition equivalence
            if !self.are_conditions_equivalent(&condition_registry, domain, conditions, neighbour_metrics).await? {
                continue;
            }
            
            // Check for metric contradictions
            if let Some(_contradiction) = self.check_metric_contradiction(metrics, neighbour_metrics).await? {
                // Check if contradicts edge already exists
                if !self.contradicts_edge_exists(new_atom_id, &neighbour.atom_id).await? {
                    contradictions.push(ContradictionInfo {
                        atom_id: neighbour.atom_id.clone(),
                        new_disagreement: 0.0, // Will be calculated below
                    });
                }
            }
        }
        
        // Calculate new disagreement values
        for contradiction in &mut contradictions {
            let (contradicts_edges, total_edges) = self.count_edge_types(&contradiction.atom_id).await?;
            contradiction.new_disagreement = disagreement(contradicts_edges, total_edges);
        }
        
        Ok(contradictions)
    }

    /// Check if two condition objects are equivalent using the condition registry
    async fn are_conditions_equivalent(
        &self,
        _condition_registry: &ConditionRegistry,
        _domain: &str,
        conditions1: &Value,
        conditions2: &Value,
    ) -> Result<bool> {
        // For MVP, use simple equivalence check
        // In a full implementation, this would use the condition registry properly
        Ok(conditions1 == conditions2)
    }

    /// Check if metrics between two atoms contradict
    async fn check_metric_contradiction(&self, metrics1: &Value, metrics2: &Value) -> Result<Option<bool>> {
        let metrics1_obj = metrics1.as_object().ok_or_else(|| 
            MoteError::Validation("Invalid metrics format".to_string()))?;
        let metrics2_obj = metrics2.as_object().ok_or_else(|| 
            MoteError::Validation("Invalid metrics format".to_string()))?;

        for (metric_name, value1) in metrics1_obj {
            if let Some(value2) = metrics2_obj.get(metric_name) {
                if let (Some(v1), Some(v2)) = (value1.as_f64(), value2.as_f64()) {
                    let higher_better = is_higher_better(metrics1, metric_name);
                    if metrics_contradict(v1, v2, higher_better, 0.1) {
                        return Ok(Some(true));
                    }
                }
            }
        }

        Ok(Some(false))
    }

    /// Check if a contradicts edge already exists between two atoms
    async fn contradicts_edge_exists(&self, atom1_id: &str, atom2_id: &str) -> Result<bool> {
        let count: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM edges 
             WHERE ((source_id = $1 AND target_id = $2) OR (source_id = $2 AND target_id = $1))
             AND type = 'contradicts'"
        )
        .bind(atom1_id)
        .bind(atom2_id)
        .fetch_one(&self.pool)
        .await?;

        Ok(count > 0)
    }

    /// Count contradicts edges and total edges for an atom
    async fn count_edge_types(&self, atom_id: &str) -> Result<(usize, usize)> {
        let row = sqlx::query(
            "SELECT 
                COUNT(*) FILTER (WHERE type = 'contradicts') as contradicts_count,
                COUNT(*) as total_count
             FROM edges 
             WHERE source_id = $1 OR target_id = $1"
        )
        .bind(atom_id)
        .fetch_one(&self.pool)
        .await?;

        let contradicts_count: i64 = row.get("contradicts_count");
        let total_count: i64 = row.get("total_count");

        Ok((contradicts_count as usize, total_count as usize))
    }

    /// Insert contradicts edge into database and graph cache
    async fn insert_contradicts_edge(&self, atom1_id: &str, atom2_id: &str) -> Result<()> {
        // Insert into database (both directions)
        sqlx::query(
            "INSERT INTO edges (source_id, target_id, type) VALUES 
             ($1, $2, 'contradicts'), ($2, $1, 'contradicts')
             ON CONFLICT (source_id, target_id, type) DO NOTHING"
        )
        .bind(atom1_id)
        .bind(atom2_id)
        .execute(&self.pool)
        .await?;

        // Update graph cache
        let mut cache = self.graph_cache.write().await;
        cache.add_edge(atom1_id, atom2_id, EdgeType::Contradicts)?;
        cache.add_edge(atom2_id, atom1_id, EdgeType::Contradicts)?;

        Ok(())
    }

    /// Apply batch pheromone updates to the database
    async fn apply_pheromone_updates(&self, updates: Vec<(String, &str, f64)>) -> Result<()> {
        if updates.is_empty() {
            return Ok(());
        }

        // Group updates by atom_id for efficiency
        let mut atom_updates: std::collections::HashMap<String, Vec<(&str, f64)>> = std::collections::HashMap::new();
        for (atom_id, field, value) in updates {
            atom_updates.entry(atom_id).or_default().push((field, value));
        }

        // Apply updates atom by atom
        for (atom_id, field_updates) in atom_updates {
            let mut set_clauses = Vec::new();
            for (field, _) in &field_updates {
                set_clauses.push(format!("{} = $2", field));
            }
            
            // This is a simplified approach - in practice you'd want to build a more sophisticated
            // batch update using unnest or similar
            for (field, value) in field_updates {
                sqlx::query(&format!("UPDATE atoms SET {} = $1 WHERE atom_id = $2", field))
                    .bind(value)
                    .bind(&atom_id)
                    .execute(&self.pool)
                    .await?;
            }
        }

        Ok(())
    }
}

#[derive(Debug, Clone)]
struct NeighbourInfo {
    atom_id: String,
    ph_attraction: f64,
    ph_repulsion: f64,
    metrics: Option<Value>,
}

#[derive(Debug, Clone)]
struct ContradictionInfo {
    atom_id: String,
    new_disagreement: f64,
}
