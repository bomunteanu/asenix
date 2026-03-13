use crate::error::{MoteError, Result};
use crate::domain::atom::Atom;
use sqlx::PgPool;
use serde::{Deserialize, Serialize};
use tracing::{info, warn};

#[derive(Debug, Serialize, Deserialize)]
pub struct ClusterQuery {
    pub query_vector: Vec<f32>,
    pub domain: Option<String>,
    pub radius: f32,
    pub limit: usize,
    pub min_confidence: Option<f64>,
    pub lifecycle_filter: Option<Vec<String>>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct ClusterResult {
    pub atoms: Vec<Atom>,
    pub total_found: i64,
    pub search_radius: f32,
    pub query_time_ms: u64,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct ClusterStats {
    pub domain: String,
    pub atom_count: i64,
    pub avg_confidence: f64,
    pub embedding_count: i64,
    pub last_updated: chrono::DateTime<chrono::Utc>,
}

pub struct QueryCluster {
    pool: PgPool,
}

impl QueryCluster {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }

    /// Query atoms within a radius using pgvector
    pub async fn query_radius(&self, query: ClusterQuery) -> Result<ClusterResult> {
        let start_time = std::time::Instant::now();
        
        // Validate query vector
        if query.query_vector.is_empty() {
            return Err(MoteError::Internal("Query vector cannot be empty".to_string()));
        }

        // Build the base query
        let mut sql = String::from(
            r#"
            SELECT atom_id, atom_type, domain, statement, conditions, metrics, provenance,
                   author_agent_id, created_at, signature, confidence, ph_attraction,
                   ph_repulsion, ph_novelty, ph_disagreement, embedding, embedding_status,
                   repl_exact, repl_conceptual, repl_extension, traffic, lifecycle,
                   retracted, retraction_reason, ban_flag, archived, probationary, summary
            FROM atoms 
            WHERE embedding_status = 'ready'
            AND NOT retracted
            AND embedding IS NOT NULL
        "#
        );

        // Add domain filter if specified
        if let Some(ref domain) = query.domain {
            sql.push_str(&format!(" AND domain = '{}' ", domain));
        }

        // Add confidence filter if specified
        if let Some(min_confidence) = query.min_confidence {
            sql.push_str(&format!(" AND confidence >= {} ", min_confidence));
        }

        // Add lifecycle filter if specified
        if let Some(ref lifecycle_filters) = query.lifecycle_filter {
            let lifecycle_str = lifecycle_filters.join("','");
            sql.push_str(&format!(" AND lifecycle IN ('{}') ", lifecycle_str));
        }

        // Add the vector similarity search
        sql.push_str(&format!(
            " AND embedding <=> $1 < {} ",
            query.radius
        ));

        // Add ordering and limit
        sql.push_str(" ORDER BY embedding <=> $1 LIMIT ");
        sql.push_str(&query.limit.to_string());

        // Execute the query
        let atoms = sqlx::query_as(&sql)
            .bind(query.query_vector.as_slice())
            .fetch_all(&self.pool)
            .await?;

        let query_time = start_time.elapsed().as_millis() as u64;
        let total_found = atoms.len() as i64;

        let result = ClusterResult {
            atoms,
            total_found,
            search_radius: query.radius,
            query_time_ms: query_time,
        };

        info!(
            "Radius query completed: found {} atoms in {}ms with radius {}",
            result.total_found, result.query_time_ms, query.radius
        );

        Ok(result)
    }

    /// Query atoms using exact nearest neighbors
    pub async fn query_nearest(&self, query_vector: &[f32], limit: usize) -> Result<Vec<Atom>> {
        let atoms = sqlx::query_as!(
            Atom,
            r#"
            SELECT atom_id, atom_type, domain, statement, conditions, metrics, provenance,
                   author_agent_id, created_at, signature, confidence, ph_attraction,
                   ph_repulsion, ph_novelty, ph_disagreement, embedding, embedding_status,
                   repl_exact, repl_conceptual, repl_extension, traffic, lifecycle,
                   retracted, retraction_reason, ban_flag, archived, probationary, summary
            FROM atoms 
            WHERE embedding_status = 'ready'
            AND NOT retracted
            AND embedding IS NOT NULL
            ORDER BY embedding <=> $1
            LIMIT $2
            "#,
            query_vector,
            limit as i64
        )
        .fetch_all(&self.pool)
        .await?;

        Ok(atoms)
    }

    /// Get cluster statistics for a domain
    pub async fn get_cluster_stats(&self, domain: &str) -> Result<ClusterStats> {
        let stats = sqlx::query!(
            r#"
            SELECT 
                COUNT(*) as atom_count,
                AVG(confidence) as avg_confidence,
                COUNT(CASE WHEN embedding IS NOT NULL THEN 1 END) as embedding_count,
                MAX(created_at) as last_updated
            FROM atoms 
            WHERE domain = $1 
            AND NOT retracted
            "#,
            domain
        )
        .fetch_one(&self.pool)
        .await?;

        let cluster_stats = ClusterStats {
            domain: domain.to_string(),
            atom_count: stats.atom_count.unwrap_or(0),
            avg_confidence: stats.avg_confidence.unwrap_or(0.0),
            embedding_count: stats.embedding_count.unwrap_or(0),
            last_updated: stats.last_updated.unwrap_or_else(|| chrono::Utc::now()),
        };

        Ok(cluster_stats)
    }

    /// Find similar atoms to a reference atom
    pub async fn find_similar(&self, reference_atom_id: &str, limit: usize) -> Result<Vec<Atom>> {
        // First get the reference atom's embedding
        let reference = sqlx::query_scalar!(
            "SELECT embedding FROM atoms WHERE atom_id = $1 AND embedding_status = 'ready'",
            reference_atom_id
        )
        .fetch_optional(&self.pool)
        .await?;

        let Some(embedding) = reference else {
            return Err(MoteError::Internal("Reference atom not found or not ready".to_string()));
        };

        // Find similar atoms
        let similar_atoms = sqlx::query_as!(
            Atom,
            r#"
            SELECT atom_id, atom_type, domain, statement, conditions, metrics, provenance,
                   author_agent_id, created_at, signature, confidence, ph_attraction,
                   ph_repulsion, ph_novelty, ph_disagreement, embedding, embedding_status,
                   repl_exact, repl_conceptual, repl_extension, traffic, lifecycle,
                   retracted, retraction_reason, ban_flag, archived, probationary, summary
            FROM atoms 
            WHERE embedding_status = 'ready'
            AND NOT retracted
            AND embedding IS NOT NULL
            AND atom_id != $1
            ORDER BY embedding <=> $2
            LIMIT $3
            "#,
            reference_atom_id,
            embedding.as_slice(),
            limit as i64
        )
        .fetch_all(&self.pool)
        .await?;

        Ok(similar_atoms)
    }

    /// Get atoms within multiple domains (multi-domain search)
    pub async fn query_multi_domain(&self, query_vector: &[f32], domains: &[String], limit: usize) -> Result<Vec<Atom>> {
        if domains.is_empty() {
            return Err(MoteError::Internal("At least one domain must be specified".to_string()));
        }

        let domain_placeholders = domains.iter().map(|_| "?").collect::<Vec<_>>().join(",");
        let sql = format!(
            r#"
            SELECT atom_id, atom_type, domain, statement, conditions, metrics, provenance,
                   author_agent_id, created_at, signature, confidence, ph_attraction,
                   ph_repulsion, ph_novelty, ph_disagreement, embedding, embedding_status,
                   repl_exact, repl_conceptual, repl_extension, traffic, lifecycle,
                   retracted, retraction_reason, ban_flag, archived, probationary, summary
            FROM atoms 
            WHERE embedding_status = 'ready'
            AND NOT retracted
            AND embedding IS NOT NULL
            AND domain IN ({})
            ORDER BY embedding <=> $1
            LIMIT $2
            "#,
            domain_placeholders
        );

        // Build the query with dynamic domain list
        let mut query_builder = sqlx::query_as::<_, Atom>(&sql).bind(query_vector);
        
        for domain in domains {
            query_builder = query_builder.bind(domain);
        }
        query_builder = query_builder.bind(limit as i64);

        let atoms = query_builder.fetch_all(&self.pool).await?;
        Ok(atoms)
    }

    /// Get density information for a vector space region
    pub async fn get_density_info(&self, center_vector: &[f32], radius: f32) -> Result<DensityInfo> {
        let result = sqlx::query!(
            r#"
            SELECT 
                COUNT(*) as total_atoms,
                AVG(confidence) as avg_confidence,
                STDDEV(confidence) as confidence_stddev,
                COUNT(CASE WHEN lifecycle = 'core' THEN 1 END) as core_atoms,
                COUNT(CASE WHEN lifecycle = 'provisional' THEN 1 END) as provisional_atoms,
                COUNT(CASE WHEN lifecycle = 'contested' THEN 1 END) as contested_atoms
            FROM atoms 
            WHERE embedding_status = 'ready'
            AND NOT retracted
            AND embedding IS NOT NULL
            AND embedding <=> $1 < $2
            "#,
            center_vector,
            radius
        )
        .fetch_one(&self.pool)
        .await?;

        let density_info = DensityInfo {
            center_vector: center_vector.to_vec(),
            radius,
            total_atoms: result.total_atoms.unwrap_or(0),
            avg_confidence: result.avg_confidence.unwrap_or(0.0),
            confidence_stddev: result.confidence_stddev.unwrap_or(0.0),
            core_atoms: result.core_atoms.unwrap_or(0),
            provisional_atoms: result.provisional_atoms.unwrap_or(0),
            contested_atoms: result.contested_atoms.unwrap_or(0),
        };

        Ok(density_info)
    }

    /// Validate that pgvector extension is available and working
    pub async fn validate_pgvector(&self) -> Result<bool> {
        let result = sqlx::query_scalar!(
            "SELECT 1 FROM pg_extension WHERE extname = 'vector'"
        )
        .fetch_optional(&self.pool)
        .await?;

        match result {
            Some(_) => {
                // Test vector operations
                let test_vector = vec![0.1; 384];
                let _ = sqlx::query!(
                    "SELECT $1 <=> $2 as distance",
                    test_vector.as_slice(),
                    test_vector.as_slice()
                )
                .fetch_one(&self.pool)
                .await?;
                
                info!("pgvector extension is validated and working");
                Ok(true)
            }
            None => {
                warn!("pgvector extension not found");
                Ok(false)
            }
        }
    }
}

#[derive(Debug, Serialize, Deserialize)]
pub struct DensityInfo {
    pub center_vector: Vec<f32>,
    pub radius: f32,
    pub total_atoms: i64,
    pub avg_confidence: f64,
    pub confidence_stddev: f64,
    pub core_atoms: i64,
    pub provisional_atoms: i64,
    pub contested_atoms: i64,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_cluster_query_creation() {
        // This would require a test database
        // For now, just test the query structure
        let query = ClusterQuery {
            query_vector: vec![0.1; 384],
            domain: Some("test".to_string()),
            radius: 0.5,
            limit: 10,
            min_confidence: Some(0.5),
            lifecycle_filter: Some(vec!["core".to_string(), "provisional".to_string()]),
        };

        assert_eq!(query.limit, 10);
        assert_eq!(query.radius, 0.5);
        assert!(query.query_vector.len() == 384);
    }

    #[test]
    fn test_density_info_structure() {
        let density = DensityInfo {
            center_vector: vec![0.1; 384],
            radius: 0.5,
            total_atoms: 100,
            avg_confidence: 0.75,
            confidence_stddev: 0.1,
            core_atoms: 50,
            provisional_atoms: 40,
            contested_atoms: 10,
        };

        assert_eq!(density.total_atoms, 100);
        assert_eq!(density.radius, 0.5);
        assert_eq!(density.core_atoms + density.provisional_atoms + density.contested_atoms, 100);
    }

    #[test]
    fn test_cluster_stats_structure() {
        let stats = ClusterStats {
            domain: "test".to_string(),
            atom_count: 1000,
            avg_confidence: 0.75,
            embedding_count: 950,
            last_updated: chrono::Utc::now(),
        };

        assert_eq!(stats.domain, "test");
        assert_eq!(stats.atom_count, 1000);
        assert!(stats.embedding_count < stats.atom_count); // Some atoms might not have embeddings
    }
}
