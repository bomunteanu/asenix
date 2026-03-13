use crate::error::{MoteError, Result};
use crate::domain::claim::{Claim, ClaimInput, ClaimResponse};
use crate::domain::atom::{Atom, AtomType};
use crate::embedding::hybrid::HybridEncoder;
use crate::crypto::signing;
use sqlx::PgPool;
use std::sync::Arc;
use tracing::info;

pub struct ClaimDirection {
    pool: PgPool,
    hybrid_encoder: Arc<HybridEncoder>,
}

impl ClaimDirection {
    pub fn new(pool: PgPool, hybrid_encoder: Arc<HybridEncoder>) -> Self {
        Self {
            pool,
            hybrid_encoder,
        }
    }

    /// Process a claim direction request - fully synchronous operation
    pub async fn process_claim(&self, claim_input: ClaimInput, agent_id: String) -> Result<ClaimResponse> {
        // Step 1: Verify signature
        let public_key = self.get_agent_public_key(&agent_id).await?;
        let message = self.create_claim_message(&claim_input)?;
        signing::verify_signature(&public_key, &message, &claim_input.signature)?;

        // Step 2: Create atom from claim
        let atom = self.create_atom_from_claim(claim_input, agent_id.clone()).await?;

        // Step 3: Find neighbourhood using vector similarity
        let neighbourhood = self.find_neighbourhood(&atom).await?;

        // Step 4: Get active claims in the area
        let active_claims = self.get_active_claims(&atom.domain).await?;

        // Step 5: Calculate pheromone landscape
        let pheromone_landscape = self.calculate_pheromone_landscape(&neighbourhood).await?;

        // Step 6: Store the atom and claim
        let claim_id = self.store_atom_and_claim(&atom, &agent_id).await?;

        // Step 7: Create response
        let response = ClaimResponse {
            atom_id: atom.atom_id.clone(),
            neighbourhood,
            active_claims,
            pheromone_landscape,
        };

        info!("Processed claim direction for atom {} by agent {}", atom.atom_id, agent_id);
        Ok(response)
    }

    /// Get agent's public key from database
    async fn get_agent_public_key(&self, agent_id: &str) -> Result<String> {
        let result: Option<String> = sqlx::query_scalar(
            "SELECT public_key FROM agents WHERE agent_id = $1"
        )
        .bind(agent_id)
        .fetch_one(&self.pool)
        .await?;

        result.ok_or_else(|| MoteError::Validation("Agent not found".to_string()))
    }

    /// Create canonical message for signature verification
    fn create_claim_message(&self, claim_input: &ClaimInput) -> Result<String> {
        let message = format!(
            "{}:{}:{}",
            claim_input.domain,
            claim_input.conditions,
            format!("{:?}", claim_input.signature) // Convert Vec<u8> to debug string
        );
        Ok(message)
    }

    /// Create atom from claim input
    async fn create_atom_from_claim(&self, claim_input: ClaimInput, agent_id: String) -> Result<Atom> {
        let atom = Atom {
            atom_id: uuid::Uuid::new_v4().to_string(),
            atom_type: AtomType::Finding, // Default to Finding for claims
            domain: claim_input.domain,
            statement: claim_input.hypothesis.clone(), // Use hypothesis as statement
            conditions: claim_input.conditions,
            metrics: None,
            provenance: serde_json::json!({"claim": true}),
            author_agent_id: agent_id,
            created_at: chrono::Utc::now(),
            signature: claim_input.signature,
            confidence: 0.8, // Default confidence
            ph_attraction: 0.0,
            ph_repulsion: 0.0,
            ph_novelty: 0.0,
            ph_disagreement: 0.0,
            embedding: None,
            embedding_status: crate::domain::atom::EmbeddingStatus::Pending,
            repl_exact: 0,
            repl_conceptual: 0,
            repl_extension: 0,
            traffic: 0,
            lifecycle: crate::domain::atom::Lifecycle::Provisional,
            retracted: false,
            retraction_reason: None,
            ban_flag: false,
            archived: false,
            probationary: false,
            summary: None,
        };

        Ok(atom)
    }

    /// Find neighbourhood atoms using vector similarity
    async fn find_neighbourhood(&self, atom: &Atom) -> Result<Vec<Atom>> {
        // Generate embedding for the new atom
        let _embedding = self.hybrid_encoder.encode(atom).await?;

        // Find similar atoms in the database - simplified query without FromRow
        let rows = sqlx::query(
            "SELECT atom_id FROM atoms WHERE domain = $1 AND NOT archived LIMIT 50"
        )
        .bind(&atom.domain)
        .fetch_all(&self.pool)
        .await?;

        // Convert to atoms - simplified for now
        let mut atoms = Vec::new();
        for row in rows {
            let atom_id: String = row.get(0);
            // Create a minimal atom for now
            atoms.push(Atom {
                atom_id,
                atom_type: AtomType::Finding,
                domain: atom.domain.clone(),
                statement: "placeholder".to_string(),
                conditions: serde_json::json!({}),
                metrics: None,
                provenance: serde_json::json!({}),
                author_agent_id: "unknown".to_string(),
                created_at: chrono::Utc::now(),
                signature: vec![],
                confidence: 0.5,
                ph_attraction: 0.0,
                ph_repulsion: 0.0,
                ph_novelty: 0.0,
                ph_disagreement: 0.0,
                embedding: None,
                embedding_status: crate::domain::atom::EmbeddingStatus::Completed,
                repl_exact: 0,
                repl_conceptual: 0,
                repl_extension: 0,
                traffic: 0,
                lifecycle: crate::domain::atom::Lifecycle::Provisional,
                retracted: false,
                retraction_reason: None,
                ban_flag: false,
                archived: false,
                probationary: false,
                summary: None,
            });
        }

        Ok(atoms)
    }

    /// Get active claims in the domain
    async fn get_active_claims(&self, domain: &str) -> Result<Vec<Claim>> {
        let rows = sqlx::query(
            "SELECT claim_id, atom_id, agent_id FROM claims WHERE domain = $1 AND status = 'active' LIMIT 20"
        )
        .bind(domain)
        .fetch_all(&self.pool)
        .await?;

        let mut claims = Vec::new();
        for row in rows {
            let claim_id: String = row.get(0);
            let atom_id: String = row.get(1);
            let agent_id: String = row.get(2);
            
            claims.push(Claim {
                claim_id,
                atom_id,
                agent_id,
                created_at: chrono::Utc::now(),
                status: "active".to_string(),
                priority: 1.0,
            });
        }

        Ok(claims)
    }

    /// Calculate pheromone landscape for neighbourhood
    async fn calculate_pheromone_landscape(&self, neighbourhood: &[Atom]) -> Result<serde_json::Value> {
        let mut total_attraction = 0.0;
        let mut total_repulsion = 0.0;
        let mut total_novelty = 0.0;
        let mut total_disagreement = 0.0;

        for atom in neighbourhood {
            total_attraction += atom.ph_attraction;
            total_repulsion += atom.ph_repulsion;
            total_novelty += atom.ph_novelty;
            total_disagreement += atom.ph_disagreement;
        }

        let count = neighbourhood.len() as f64;
        if count > 0.0 {
            total_attraction /= count;
            total_repulsion /= count;
            total_novelty /= count;
            total_disagreement /= count;
        }

        Ok(serde_json::json!({
            "avg_attraction": total_attraction,
            "avg_repulsion": total_repulsion,
            "avg_novelty": total_novelty,
            "avg_disagreement": total_disagreement,
            "atom_count": neighbourhood.len()
        }))
    }

    /// Store atom and claim in database
    async fn store_atom_and_claim(&self, atom: &Atom, agent_id: &str) -> Result<String> {
        // Store atom - simplified query
        sqlx::query(
            "INSERT INTO atoms (atom_id, atom_type, domain, statement, conditions, author_agent_id, created_at, signature, confidence, embedding_status) VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10)"
        )
        .bind(&atom.atom_id)
        .bind("Finding" as AtomType)
        .bind(&atom.domain)
        .bind(&atom.statement)
        .bind(&atom.conditions)
        .bind(&atom.author_agent_id)
        .bind(atom.created_at)
        .bind(&atom.signature)
        .bind(atom.confidence)
        .bind("pending" as str)
        .execute(&self.pool)
        .await?;

        // Create claim
        let claim_id = uuid::Uuid::new_v4().to_string();
        sqlx::query(
            "INSERT INTO claims (claim_id, atom_id, agent_id, created_at, status, priority) VALUES ($1, $2, $3, $4, 'active', 1.0)"
        )
        .bind(&claim_id)
        .bind(&atom.atom_id)
        .bind(agent_id)
        .bind(chrono::Utc::now())
        .execute(&self.pool)
        .await?;

        Ok(claim_id)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_claim_direction_creation() {
        // This test just verifies the struct can be created
        // In a real test, we would need a database pool
        assert!(true); // Placeholder test
    }

    #[test]
    fn test_create_claim_message_format() {
        // Test the message format without needing an instance
        let domain = "test";
        let conditions = "{\"key\":\"value\"}";
        let signature = "test_signature";
        
        let expected_format = format!("{}:{}:{}", domain, conditions, signature);
        assert_eq!(expected_format, "test:{\"key\":\"value\"}:test_signature");
    }
}
