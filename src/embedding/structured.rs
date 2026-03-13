use crate::domain::condition::ConditionRegistry;
use crate::error::Result;
use crate::domain::atom::Atom;
use std::sync::Arc;
use std::f32::consts::PI;

pub struct StructuredEncoder {
    registry: Arc<ConditionRegistry>,
    reserved_dims: usize,
    dims_per_numeric_key: usize,
    dims_per_categorical_key: usize,
    max_log_value: f32,
}

impl StructuredEncoder {
    pub fn new(registry: Arc<ConditionRegistry>, reserved_dims: usize, dims_per_numeric_key: usize, dims_per_categorical_key: usize) -> Result<Self> {
        Ok(Self {
            registry,
            reserved_dims,
            dims_per_numeric_key,
            dims_per_categorical_key,
            max_log_value: 10.0_f32,
        })
    }

    pub fn encode(&self, atom: &Atom) -> Result<Vec<f32>> {
        let mut vector = Vec::with_capacity(self.reserved_dims);
        
        // Encode atom type (2 dimensions)
        let atom_type_vector = self.encode_atom_type(&atom.atom_type);
        vector.extend(atom_type_vector);
        
        // Encode conditions using registry-based approach
        let conditions_vector = self.encode_conditions_with_registry(&atom.conditions)?;
        vector.extend(conditions_vector);
        
        // Pad to reserved dimensions if needed
        while vector.len() < self.reserved_dims {
            vector.push(0.0);
        }
        
        // Truncate if we exceed reserved dimensions (overflow handling)
        vector.truncate(self.reserved_dims);
        
        Ok(vector)
    }

    fn encode_atom_type(&self, atom_type: &crate::domain::atom::AtomType) -> Vec<f32> {
        let type_string = format!("{:?}", atom_type);
        let mut hash = self.hash_string(&type_string);
        let mut vector = Vec::with_capacity(2);
        
        for _ in 0..2 {
            hash = hash.wrapping_mul(1103515245).wrapping_add(12345);
            vector.push((hash as f32 / u32::MAX as f32) * 2.0 - 1.0);
        }
        
        vector
    }

    fn encode_conditions_with_registry(&self, conditions: &serde_json::Value) -> Result<Vec<f32>> {
        let mut vector = Vec::new();
        
        if let Some(obj) = conditions.as_object() {
            for (key, value) in obj {
                // Check if this key is in the condition registry for this domain
                // For now, we'll encode all keys but in a real implementation
                // this would filter based on the registry
                
                if let Some(num) = value.as_f64() {
                    // Numeric key: log-scale encoding in fixed-width subvector
                    let encoded = self.encode_numeric_key(key, num);
                    vector.extend(encoded);
                } else if let Some(str_val) = value.as_str() {
                    // Categorical key: deterministic hash → unit vector
                    let encoded = self.encode_categorical_key(key, str_val);
                    vector.extend(encoded);
                } else {
                    // Unknown type: ignore safely (no panic)
                    continue;
                }
            }
        }
        
        Ok(vector)
    }

    fn encode_numeric_key(&self, key: &str, value: f64) -> Vec<f32> {
        let mut vector = Vec::with_capacity(self.dims_per_numeric_key);
        
        // Use log-scale encoding for numeric values
        let log_value = if value > 0.0 {
            (value.ln() / 10.0_f64.ln()) as f32 // Normalize by log(10)
        } else if value < 0.0 {
            -((-value).ln() / 10.0_f64.ln()) as f32
        } else {
            0.0
        };
        
        // Create fixed-width subvector with log-scale encoding
        let key_hash = self.hash_string(key);
        for i in 0..self.dims_per_numeric_key {
            let phase = (key_hash.wrapping_mul(i as u32 + 1) % 360) as f32 * PI / 180.0;
            vector.push(log_value * phase.cos());
        }
        
        vector
    }

    fn encode_categorical_key(&self, key: &str, value: &str) -> Vec<f32> {
        let mut vector = Vec::with_capacity(self.dims_per_categorical_key);
        
        // Create deterministic hash from key+value combination
        let combined = format!("{}:{}", key, value);
        let hash = self.hash_string(&combined);
        
        // Generate unit vector from deterministic hash
        for i in 0..self.dims_per_categorical_key {
            let angle = (hash.wrapping_mul(i as u32 + 1) % 360) as f32 * 2.0 * PI / 360.0;
            vector.push(angle.cos());
        }
        
        // Normalize to unit vector
        let magnitude: f32 = vector.iter().map(|x| x * x).sum::<f32>().sqrt();
        if magnitude > 0.0 {
            for val in &mut vector {
                *val /= magnitude;
            }
        }
        
        vector
    }


    fn hash_string(&self, s: &str) -> u32 {
        let mut hash = 0u32;
        for byte in s.bytes() {
            hash = hash.wrapping_mul(31).wrapping_add(byte as u32);
        }
        hash
    }

    pub fn vector_size(&self) -> usize {
        self.reserved_dims
    }
}

impl Clone for StructuredEncoder {
    fn clone(&self) -> Self {
        Self {
            registry: self.registry.clone(),
            reserved_dims: self.reserved_dims,
            dims_per_numeric_key: self.dims_per_numeric_key,
            dims_per_categorical_key: self.dims_per_categorical_key,
            max_log_value: self.max_log_value,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::atom::AtomType;
    use crate::domain::atom::Lifecycle;
    use serde_json::json;

    #[test]
    fn test_determinism() {
        let registry = Arc::new(ConditionRegistry::new());
        let encoder = StructuredEncoder::new(registry, 10, 2, 4).unwrap();
        
        let conditions = json!({
            "model_params": 7000000000.0,
            "dataset": "mmlu"
        });
        
        let atom = crate::domain::atom::Atom {
            atom_id: "test_atom".to_string(),
            atom_type: AtomType::Finding,
            domain: "test".to_string(),
            statement: "Test statement".to_string(),
            conditions: conditions.clone(),
            metrics: None,
            provenance: json!({}),
            author_agent_id: "test_author".to_string(),
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
            lifecycle: Lifecycle::Provisional,
            retracted: false,
            retraction_reason: None,
            ban_flag: false,
            archived: false,
            probationary: false,
            summary: None,
        };
        
        let vector1 = encoder.encode(&atom).unwrap();
        let vector2 = encoder.encode(&atom).unwrap();
        
        assert_eq!(vector1.len(), vector2.len());
        for i in 0..vector1.len() {
            assert!((vector1[i] - vector2[i]).abs() < 1e-6);
        }
    }

    #[test]
    fn test_vector_size() {
        let registry = Arc::new(ConditionRegistry::new());
        let encoder = StructuredEncoder::new(registry, 10, 2, 4).unwrap();
        
        assert_eq!(encoder.vector_size(), 10);
    }
}
