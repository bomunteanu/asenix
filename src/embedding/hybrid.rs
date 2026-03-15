use crate::error::{MoteError, Result};
use crate::embedding::semantic::SemanticEncoder;
use crate::embedding::structured::StructuredEncoder;
use tracing::info;

pub struct HybridEncoder {
    semantic_encoder: SemanticEncoder,
    structured_encoder: StructuredEncoder,
    semantic_weight: f32,
    structured_weight: f32,
}

impl HybridEncoder {
    pub fn new(
        semantic_encoder: SemanticEncoder,
        structured_encoder: StructuredEncoder,
    ) -> Result<Self> {
        // Default weights - can be made configurable
        let semantic_weight = 0.7;
        let structured_weight = 0.3;

        Ok(Self {
            semantic_encoder,
            structured_encoder,
            semantic_weight,
            structured_weight,
        })
    }

    /// Create hybrid encoder with custom weights
    pub fn new_with_weights(
        semantic_encoder: SemanticEncoder,
        structured_encoder: StructuredEncoder,
        semantic_weight: f32,
        structured_weight: f32,
    ) -> Result<Self> {
        // Validate weights sum to 1.0
        let total_weight = semantic_weight + structured_weight;
        if (total_weight - 1.0).abs() > 0.001 {
            return Err(MoteError::Internal(
                format!("Semantic weight ({}) and structured weight ({}) must sum to 1.0, got {}", 
                    semantic_weight, structured_weight, total_weight)
            ));
        }

        Ok(Self {
            semantic_encoder,
            structured_encoder,
            semantic_weight,
            structured_weight,
        })
    }

    /// Encode atom into hybrid vector combining semantic and structured embeddings
    pub async fn encode(&self, atom: &crate::domain::atom::Atom) -> Result<Vec<f32>> {
        // Get semantic embedding
        let semantic_vector = if self.semantic_encoder.is_configured() {
            match self.semantic_encoder.encode(&atom.statement).await {
                Ok(vector) => vector,
                Err(e) => {
                    tracing::warn!("Failed to get semantic embedding, using fallback: {}", e);
                    // Create fallback semantic vector based on statement hash
                    self.create_fallback_semantic_vector(&atom.statement)?
                }
            }
        } else {
            // Use fallback if not configured
            self.create_fallback_semantic_vector(&atom.statement)?
        };

        // Get structured embedding
        let structured_vector = self.structured_encoder.encode(atom)?;

        // Combine vectors
        let hybrid_vector = self.combine_vectors(&semantic_vector, &structured_vector)?;

        info!(
            "Created hybrid embedding for atom {}: {} dimensions (semantic: {}, structured: {})",
            atom.atom_id,
            hybrid_vector.len(),
            semantic_vector.len(),
            structured_vector.len()
        );

        Ok(hybrid_vector)
    }

    /// Combine semantic and structured vectors using concatenation (not weighted average)
    fn combine_vectors(&self, semantic: &[f32], structured: &[f32]) -> Result<Vec<f32>> {
        // Concatenate semantic and structured vectors per spec
        let mut combined = Vec::with_capacity(semantic.len() + structured.len());
        combined.extend_from_slice(semantic);
        combined.extend_from_slice(structured);
        
        // Normalize final vector
        let magnitude: f32 = combined.iter().map(|x| x * x).sum::<f32>().sqrt();
        if magnitude > 0.0 {
            for val in &mut combined {
                *val /= magnitude;
            }
        }
        
        Ok(combined)
    }

    /// Create fallback semantic vector based on statement hash when API is unavailable
    fn create_fallback_semantic_vector(&self, statement: &str) -> Result<Vec<f32>> {
        let mut vector = Vec::with_capacity(384); // Common embedding dimension
        
        let mut hash = self.hash_string(statement);
        
        // Generate pseudo-embedding from hash
        for _ in 0..384 {
            hash = hash.wrapping_mul(1103515245).wrapping_add(12345);
            let normalized = (hash as f32 / u32::MAX as f32) * 2.0 - 1.0;
            vector.push(normalized);
        }

        Ok(vector)
    }

    /// Simple hash function for fallback generation
    fn hash_string(&self, s: &str) -> u32 {
        let mut hash = 0u32;
        for byte in s.bytes() {
            hash = hash.wrapping_mul(31).wrapping_add(byte as u32);
        }
        hash
    }

    /// Get the expected dimension of the hybrid vector
    pub fn get_dimension(&self) -> usize {
        // Use fallback dimension for now since we can't call async here
        let semantic_dim = 384; // Fallback dimension
        let structured_dim = self.structured_encoder.vector_size();
        
        // Return concatenated dimension (semantic + structured)
        semantic_dim + structured_dim
    }

    /// Check if semantic encoder is properly configured
    pub fn is_semantic_configured(&self) -> bool {
        self.semantic_encoder.is_configured()
    }

    /// Get current weights
    pub fn get_weights(&self) -> (f32, f32) {
        (self.semantic_weight, self.structured_weight)
    }

    /// Update weights
    pub fn update_weights(&mut self, semantic_weight: f32, structured_weight: f32) -> Result<()> {
        let total_weight = semantic_weight + structured_weight;
        if (total_weight - 1.0).abs() > 0.001 {
            return Err(MoteError::Internal(
                format!("Weights must sum to 1.0, got {}", total_weight)
            ));
        }

        self.semantic_weight = semantic_weight;
        self.structured_weight = structured_weight;
        Ok(())
    }
}

impl Clone for HybridEncoder {
    fn clone(&self) -> Self {
        Self {
            semantic_encoder: self.semantic_encoder.clone(),
            structured_encoder: self.structured_encoder.clone(),
            semantic_weight: self.semantic_weight,
            structured_weight: self.structured_weight,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::condition::ConditionRegistry;
    use serde_json::json;
    use std::sync::Arc;

    #[test]
    fn test_hybrid_encoder_creation() {
        let registry = Arc::new(ConditionRegistry::new());
        let semantic_encoder = SemanticEncoder::new().unwrap();
        let _structured_encoder = StructuredEncoder::new(registry.clone(), 10, 2, 4).unwrap();
        
        let encoder = HybridEncoder::new(
            semantic_encoder,
            StructuredEncoder::new(registry, 10, 2, 4).unwrap()
        );
        assert!(encoder.is_ok());
        
        let encoder = encoder.unwrap();
        assert_eq!(encoder.get_weights(), (0.7, 0.3));
    }

    #[test]
    fn test_hybrid_encoder_custom_weights() {
        let registry = Arc::new(ConditionRegistry::new());
        let semantic_encoder = SemanticEncoder::new().unwrap();
        let structured_encoder = StructuredEncoder::new(registry, 10, 2, 4).unwrap();
        
        let encoder = HybridEncoder::new_with_weights(
            semantic_encoder,
            structured_encoder,
            0.8,
            0.2,
        );
        assert!(encoder.is_ok());
        
        let encoder = encoder.unwrap();
        assert_eq!(encoder.get_weights(), (0.8, 0.2));
    }

    #[test]
    fn test_hybrid_encoder_invalid_weights() {
        let registry = Arc::new(ConditionRegistry::new());
        let semantic_encoder = SemanticEncoder::new().unwrap();
        let structured_encoder = StructuredEncoder::new(registry, 10, 2, 4).unwrap();
        
        let encoder = HybridEncoder::new_with_weights(
            semantic_encoder, 
            structured_encoder, 
            0.9, 
            0.2  // Sum = 1.1, invalid
        );
        assert!(encoder.is_err());
    }

    #[test]
    fn test_combine_vectors() {
        let registry = Arc::new(ConditionRegistry::new());
        let semantic_encoder = SemanticEncoder::new().unwrap();
        let structured_encoder = StructuredEncoder::new(registry, 10, 2, 4).unwrap();
        
        let encoder = HybridEncoder::new(semantic_encoder, structured_encoder).unwrap();
        
        let semantic = vec![1.0, 2.0, 3.0];
        let structured = vec![0.5, 1.5, 2.5];
        
        let combined = encoder.combine_vectors(&semantic, &structured).unwrap();
        
        // Expected: concatenated vector [1.0, 2.0, 3.0, 0.5, 1.5, 2.5], then normalized
        assert_eq!(combined.len(), 6);
        
        // Calculate expected normalized values
        let concatenated = [1.0, 2.0, 3.0, 0.5, 1.5, 2.5];
        let magnitude: f32 = concatenated.iter().map(|x| x * x).sum::<f32>().sqrt();
        let expected_0 = 1.0 / magnitude;
        let expected_1 = 2.0 / magnitude;
        let expected_2 = 3.0 / magnitude;
        let expected_3 = 0.5 / magnitude;
        let expected_4 = 1.5 / magnitude;
        let expected_5 = 2.5 / magnitude;
        
        // Check that semantic part is preserved at start (normalized)
        assert!((combined[0] - expected_0).abs() < 1e-6);
        assert!((combined[1] - expected_1).abs() < 1e-6);
        assert!((combined[2] - expected_2).abs() < 1e-6);
        // Check that structured part follows (normalized)
        assert!((combined[3] - expected_3).abs() < 1e-6);
        assert!((combined[4] - expected_4).abs() < 1e-6);
        assert!((combined[5] - expected_5).abs() < 1e-6);
    }

    #[test]
    fn test_combine_vectors_different_lengths() {
        let registry = Arc::new(ConditionRegistry::new());
        let semantic_encoder = SemanticEncoder::new().unwrap();
        let structured_encoder = StructuredEncoder::new(registry, 10, 2, 4).unwrap();
        
        let encoder = HybridEncoder::new(semantic_encoder, structured_encoder).unwrap();
        
        let semantic = vec![1.0, 2.0, 3.0, 4.0];
        let structured = vec![0.5, 1.5];
        
        let combined = encoder.combine_vectors(&semantic, &structured).unwrap();
        
        // Expected: concatenated vector [1.0, 2.0, 3.0, 4.0, 0.5, 1.5], then normalized
        assert_eq!(combined.len(), 6);
        
        // Calculate expected normalized values
        let concatenated = [1.0, 2.0, 3.0, 4.0, 0.5, 1.5];
        let magnitude: f32 = concatenated.iter().map(|x| x * x).sum::<f32>().sqrt();
        let expected_0 = 1.0 / magnitude;
        let expected_1 = 2.0 / magnitude;
        let expected_2 = 3.0 / magnitude;
        let expected_3 = 4.0 / magnitude;
        let expected_4 = 0.5 / magnitude;
        let expected_5 = 1.5 / magnitude;
        
        assert!((combined[0] - expected_0).abs() < 1e-6);
        assert!((combined[1] - expected_1).abs() < 1e-6);
        assert!((combined[2] - expected_2).abs() < 1e-6);
        assert!((combined[3] - expected_3).abs() < 1e-6);
        assert!((combined[4] - expected_4).abs() < 1e-6);
        assert!((combined[5] - expected_5).abs() < 1e-6);
    }

    #[test]
    fn test_fallback_semantic_vector() {
        let registry = Arc::new(ConditionRegistry::new());
        let semantic_encoder = SemanticEncoder::new().unwrap();
        let structured_encoder = StructuredEncoder::new(registry, 10, 2, 4).unwrap();
        
        let encoder = HybridEncoder::new(semantic_encoder, structured_encoder).unwrap();
        
        let vector = encoder.create_fallback_semantic_vector("test statement").unwrap();
        assert!(!vector.is_empty());
        
        // Check that vector contains values in [-1, 1] range
        for &val in &vector {
            assert!((-1.0..=1.0).contains(&val));
        }
    }

    #[test]
    fn test_fallback_semantic_vector_determinism() {
        let registry = Arc::new(ConditionRegistry::new());
        let semantic_encoder = SemanticEncoder::new().unwrap();
        let structured_encoder = StructuredEncoder::new(registry, 10, 2, 4).unwrap();
        
        let encoder = HybridEncoder::new(semantic_encoder, structured_encoder).unwrap();
        
        let vector1 = encoder.create_fallback_semantic_vector("test statement").unwrap();
        let vector2 = encoder.create_fallback_semantic_vector("test statement").unwrap();
        
        // Check that fallback vectors are deterministic
        assert_eq!(vector1.len(), vector2.len());
        for i in 0..vector1.len() {
            assert!((vector1[i] - vector2[i]).abs() < 1e-6);
        }
    }

    #[test]
    fn test_fallback_semantic_vector_different_inputs() {
        let registry = Arc::new(ConditionRegistry::new());
        let semantic_encoder = SemanticEncoder::new().unwrap();
        let structured_encoder = StructuredEncoder::new(registry, 10, 2, 4).unwrap();
        
        let encoder = HybridEncoder::new(semantic_encoder, structured_encoder).unwrap();
        
        let vector1 = encoder.create_fallback_semantic_vector("test statement 1").unwrap();
        let vector2 = encoder.create_fallback_semantic_vector("test statement 2").unwrap();
        
        // Check that different inputs produce different fallback vectors
        assert_ne!(vector1, vector2);
    }

    #[tokio::test]
    async fn test_encode_with_fallback() {
        let registry = Arc::new(ConditionRegistry::new());
        let semantic_encoder = SemanticEncoder::new().unwrap();
        let structured_encoder = StructuredEncoder::new(registry, 10, 2, 4).unwrap();
        
        let encoder = HybridEncoder::new(semantic_encoder, structured_encoder).unwrap();
        
        // Create a test atom
        let atom = crate::domain::atom::Atom {
            atom_id: "test_atom".to_string(),
            atom_type: crate::domain::atom::AtomType::Finding,
            domain: "test".to_string(),
            project_id: None,
            statement: "Test statement for encoding".to_string(),
            conditions: json!({"model_params": 1000000000.0}),
            metrics: None,
            provenance: json!({}),
            author_agent_id: "test_agent".to_string(),
            created_at: chrono::Utc::now(),
            signature: vec![],
            artifact_tree_hash: None,
            confidence: 0.5,
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
        
        // Encode should work even if semantic encoder is not configured
        let result = encoder.encode(&atom).await;
        assert!(result.is_ok());
        
        let embedding = result.unwrap();
        assert!(!embedding.is_empty());
    }

    #[test]
    fn test_get_dimension() {
        let registry = Arc::new(ConditionRegistry::new());
        let semantic_encoder = SemanticEncoder::new().unwrap();
        let structured_encoder = StructuredEncoder::new(registry, 10, 2, 4).unwrap();
        
        let encoder = HybridEncoder::new(semantic_encoder, structured_encoder).unwrap();
        
        let dimension = encoder.get_dimension();
        assert_eq!(dimension, 394); // 384 (semantic) + 10 (structured)
    }

    #[test]
    fn test_zero_vector_handling() {
        let semantic_vector = vec![0.0, 0.0, 0.0, 0.0];
        let structured_vector = vec![1.0, 2.0, 3.0, 4.0];
        
        let registry = Arc::new(ConditionRegistry::new());
        let semantic_encoder = SemanticEncoder::new().unwrap();
        let structured_encoder = StructuredEncoder::new(registry, 10, 2, 4).unwrap();
        
        let encoder = HybridEncoder::new(semantic_encoder, structured_encoder).unwrap();
        let result = encoder.combine_vectors(&semantic_vector, &structured_vector).unwrap();
        
        // Expected: concatenated vector [0.0, 0.0, 0.0, 0.0, 1.0, 2.0, 3.0, 4.0], then normalized
        assert_eq!(result.len(), 8);
        
        // Calculate expected normalized values (only structured part contributes)
        let concatenated = [0.0, 0.0, 0.0, 0.0, 1.0, 2.0, 3.0, 4.0];
        let magnitude: f32 = concatenated.iter().map(|x| x * x).sum::<f32>().sqrt();
        let expected_0 = 0.0 / magnitude; // semantic zeros
        let expected_1 = 0.0 / magnitude;
        let expected_2 = 0.0 / magnitude;
        let expected_3 = 0.0 / magnitude;
        let expected_4 = 1.0 / magnitude; // structured part
        let expected_5 = 2.0 / magnitude;
        let expected_6 = 3.0 / magnitude;
        let expected_7 = 4.0 / magnitude;
        
        assert!((result[0] - expected_0).abs() < 1e-6);
        assert!((result[1] - expected_1).abs() < 1e-6);
        assert!((result[2] - expected_2).abs() < 1e-6);
        assert!((result[3] - expected_3).abs() < 1e-6);
        assert!((result[4] - expected_4).abs() < 1e-6);
        assert!((result[5] - expected_5).abs() < 1e-6);
        assert!((result[6] - expected_6).abs() < 1e-6);
        assert!((result[7] - expected_7).abs() < 1e-6);
    }

    #[test]
    fn test_both_zero_vectors() {
        let semantic_vector = vec![0.0, 0.0, 0.0, 0.0];
        let structured_vector = vec![0.0, 0.0, 0.0, 0.0];
        
        let registry = Arc::new(ConditionRegistry::new());
        let semantic_encoder = SemanticEncoder::new().unwrap();
        let structured_encoder = StructuredEncoder::new(registry, 10, 2, 4).unwrap();
        
        let encoder = HybridEncoder::new(semantic_encoder, structured_encoder).unwrap();
        let result = encoder.combine_vectors(&semantic_vector, &structured_vector).unwrap();
        
        // Expected: concatenated vector [0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0], then normalized
        // When all zeros, normalization doesn't change anything
        assert_eq!(result.len(), 8);
        assert_eq!(result, vec![0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0]);
    }

    #[test]
    fn test_fallback_vector_determinism() {
        let registry = Arc::new(ConditionRegistry::new());
        let semantic_encoder = SemanticEncoder::new().unwrap();
        let structured_encoder = StructuredEncoder::new(registry, 10, 2, 4).unwrap();
        
        let encoder = HybridEncoder::new(semantic_encoder, structured_encoder).unwrap();
        
        let vector1 = encoder.create_fallback_semantic_vector("test").unwrap();
        let vector2 = encoder.create_fallback_semantic_vector("test").unwrap();
        
        assert_eq!(vector1, vector2);
    }

    #[test]
    fn test_fallback_vector_different_inputs() {
        let registry = Arc::new(ConditionRegistry::new());
        let semantic_encoder = SemanticEncoder::new().unwrap();
        let structured_encoder = StructuredEncoder::new(registry, 10, 2, 4).unwrap();
        
        let encoder = HybridEncoder::new(semantic_encoder, structured_encoder).unwrap();
        
        let fallback1 = encoder.create_fallback_semantic_vector("test statement 1").unwrap();
        let fallback2 = encoder.create_fallback_semantic_vector("test statement 2").unwrap();
        
        // Check that different inputs produce different fallback vectors
        assert_ne!(fallback1, fallback2);
    }
}
