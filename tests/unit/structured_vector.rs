//! Unit tests for structured vector encoding
//! 
//! Tests the encoder production of fixed-width subvectors for each key type,
//! reserved dimension enforcement, and determinism.

use serde_json::json;
use mote::domain::condition::{ConditionRegistry, ValueType, ConditionValue};
use mote::embedding::structured::StructuredVectorEncoder;

fn create_test_encoder() -> StructuredVectorEncoder {
    StructuredVectorEncoder::new(
        10, // reserved_dims
        2,  // dims_per_numeric_key
        4,  // dims_per_categorical_key
    )
}

fn create_test_registry() -> ConditionRegistry {
    let mut registry = ConditionRegistry::new();
    
    // Register test conditions
    registry.register("temperature".to_string(), ValueType::Numeric, "Temperature in Celsius".to_string());
    registry.register("pressure".to_string(), ValueType::Numeric, "Pressure in kPa".to_string());
    registry.register("location".to_string(), ValueType::Categorical, "Location identifier".to_string());
    registry.register("experiment_type".to_string(), ValueType::Categorical, "Type of experiment".to_string());
    registry.register("confidence".to_string(), ValueType::Numeric, "Confidence score".to_string());
    registry.register("device_id".to_string(), ValueType::Categorical, "Device identifier".to_string());
    
    registry
}

#[tokio::test]
async fn test_numeric_log_scale_encoding() {
    let encoder = create_test_encoder();
    let registry = create_test_registry();
    
    let conditions = json!({
        "temperature": 25.0,
        "pressure": 101.3
    });
    
    let vector = encoder.encode(&conditions, &registry).unwrap();
    
    // Should have exactly reserved_dims length
    assert_eq!(vector.len(), 10);
    
    // Numeric keys should use dims_per_numeric_key each (2 dims each)
    // So temperature and pressure should use 4 dims total
    // The remaining 6 dims should be zeros for unused keys
    
    // Values should be normalized and log-scaled
    // We don't test exact values here as they depend on the specific encoding algorithm
    // but we verify the structure is correct
    assert!(vector.iter().any(|&x| x != 0.0), "Should have non-zero values for numeric keys");
}

#[tokio::test]
async fn test_categorical_hash_encoding() {
    let encoder = create_test_encoder();
    let registry = create_test_registry();
    
    let conditions = json!({
        "location": "lab_a",
        "experiment_type": "controlled"
    });
    
    let vector = encoder.encode(&conditions, &registry).unwrap();
    
    assert_eq!(vector.len(), 10);
    
    // Categorical keys should use dims_per_categorical_key each (4 dims each)
    // So location and experiment_type should use 8 dims total
    // The remaining 2 dims should be zeros for unused keys
    
    assert!(vector.iter().any(|&x| x != 0.0), "Should have non-zero values for categorical keys");
    
    // Categorical vectors should be normalized (unit vectors)
    // Check that categorical subvectors have unit length
    let location_start = 0; // First key in registry
    let location_end = location_start + 4;
    let location_subvector = &vector[location_start..location_end];
    let location_norm: f32 = location_subvector.iter().map(|&x| x * x).sum::<f32>().sqrt();
    assert!((location_norm - 1.0).abs() < 0.001, "Categorical subvector should be normalized");
}

#[tokio::test]
async fn test_mixed_numeric_categorical() {
    let encoder = create_test_encoder();
    let registry = create_test_registry();
    
    let conditions = json!({
        "temperature": 25.0,        // numeric - 2 dims
        "location": "lab_a",       // categorical - 4 dims
        "confidence": 0.95          // numeric - 2 dims
    });
    
    let vector = encoder.encode(&conditions, &registry).unwrap();
    
    assert_eq!(vector.len(), 10);
    
    // Should have non-zero values for all provided keys
    let non_zero_count = vector.iter().filter(|&&x| x != 0.0).count();
    assert!(non_zero_count >= 6, "Should have non-zero values for mixed keys");
}

#[tokio::test]
async fn test_reserved_dimension_enforcement() {
    let encoder = create_test_encoder();
    let registry = create_test_registry();
    
    // Create conditions with more registered keys than reserved dimensions allow
    let conditions = json!({
        "temperature": 25.0,     // 2 dims
        "pressure": 101.3,       // 2 dims  
        "location": "lab_a",    // 4 dims
        "experiment_type": "controlled", // 4 dims
        "confidence": 0.95,      // 2 dims
        "device_id": "dev_123"   // 4 dims - this should exceed capacity
    });
    
    let vector = encoder.encode(&conditions, &registry).unwrap();
    
    // Should still be exactly reserved_dims length
    assert_eq!(vector.len(), 10);
    
    // Some keys should be skipped due to capacity limits
    // The exact behavior depends on implementation, but length should be fixed
}

#[tokio::test]
async fn test_unregistered_keys_mapped_to_zero() {
    let encoder = create_test_encoder();
    let registry = create_test_registry();
    
    let conditions = json!({
        "temperature": 25.0,        // registered
        "unregistered_key": "value" // not registered
    });
    
    let vector = encoder.encode(&conditions, &registry).unwrap();
    
    assert_eq!(vector.len(), 10);
    
    // Unregistered keys should be ignored or mapped to zero
    // Only registered keys should contribute to the vector
    let non_zero_indices: Vec<usize> = vector
        .iter()
        .enumerate()
        .filter(|(_, &x)| x != 0.0)
        .map(|(i, _)| i)
        .collect();
    
    // Should only have non-zero values for registered keys
    assert!(non_zero_indices.len() <= 4, "Should only have non-zero for registered temperature key");
}

#[tokio::test]
async fn test_determinism_same_inputs() {
    let encoder = create_test_encoder();
    let registry = create_test_registry();
    
    let conditions = json!({
        "temperature": 25.0,
        "location": "lab_a",
        "confidence": 0.95
    });
    
    let vector1 = encoder.encode(&conditions, &registry).unwrap();
    let vector2 = encoder.encode(&conditions, &registry).unwrap();
    
    assert_eq!(vector1, vector2, "Same inputs should produce identical vectors");
}

#[tokio::test]
async fn test_determinism_different_order() {
    let encoder = create_test_encoder();
    let registry = create_test_registry();
    
    // Create conditions with different JSON order but same logical content
    let conditions1 = json!({
        "temperature": 25.0,
        "location": "lab_a",
        "confidence": 0.95
    });
    
    let conditions2 = json!({
        "location": "lab_a",
        "confidence": 0.95,
        "temperature": 25.0
    });
    
    let vector1 = encoder.encode(&conditions1, &registry).unwrap();
    let vector2 = encoder.encode(&conditions2, &registry).unwrap();
    
    // Should be identical if encoding is order-independent
    // If order matters, this test would fail - adjust based on implementation
    assert_eq!(vector1, vector2, "Different JSON order should produce identical vectors");
}

#[tokio::test]
async fn test_numeric_extreme_values() {
    let encoder = create_test_encoder();
    let registry = create_test_registry();
    
    // Test with very small and very large values
    let conditions = json!({
        "temperature": 0.001,    // very small
        "pressure": 1000000.0     // very large
    });
    
    let vector = encoder.encode(&conditions, &registry).unwrap();
    
    assert_eq!(vector.len(), 10);
    assert!(vector.iter().any(|&x| x != 0.0), "Should handle extreme values");
    
    // Values should be normalized/log-scaled to prevent overflow
    assert!(vector.iter().all(|&x| x.is_finite()), "All values should be finite");
}

#[tokio::test]
async fn test_categorical_hash_stability() {
    let encoder = create_test_encoder();
    let registry = create_test_registry();
    
    let conditions = json!({
        "location": "test_location_123"
    });
    
    let vector1 = encoder.encode(&conditions, &registry).unwrap();
    let vector2 = encoder.encode(&conditions, &registry).unwrap();
    
    assert_eq!(vector1, vector2, "Categorical hashing should be stable");
    
    // Different categorical values should produce different vectors
    let different_conditions = json!({
        "location": "different_location"
    });
    
    let different_vector = encoder.encode(&different_conditions, &registry).unwrap();
    assert_ne!(vector1, different_vector, "Different categorical values should produce different vectors");
}

#[tokio::test]
async fn test_empty_conditions() {
    let encoder = create_test_encoder();
    let registry = create_test_registry();
    
    let conditions = json!({});
    
    let vector = encoder.encode(&conditions, &registry).unwrap();
    
    assert_eq!(vector.len(), 10);
    // Should be all zeros for empty conditions
    assert!(vector.iter().all(|&x| x == 0.0), "Empty conditions should produce zero vector");
}

#[tokio::test]
async fn test_null_conditions() {
    let encoder = create_test_encoder();
    let registry = create_test_registry();
    
    let conditions = json!(null);
    
    let vector = encoder.encode(&conditions, &registry).unwrap();
    
    assert_eq!(vector.len(), 10);
    // Should be all zeros for null conditions
    assert!(vector.iter().all(|&x| x == 0.0), "Null conditions should produce zero vector");
}

#[tokio::test]
async fn test_invalid_numeric_values() {
    let encoder = create_test_encoder();
    let registry = create_test_registry();
    
    // Test with invalid numeric values
    let conditions = json!({
        "temperature": "not_a_number",
        "pressure": null
    });
    
    let vector = encoder.encode(&conditions, &registry).unwrap();
    
    assert_eq!(vector.len(), 10);
    // Invalid values should be handled gracefully (mapped to zero or ignored)
    assert!(vector.iter().all(|&x| x.is_finite()), "All values should be finite");
}

#[tokio::test]
async fn test_categorical_normalization() {
    let encoder = create_test_encoder();
    let registry = create_test_registry();
    
    let conditions = json!({
        "location": "test_location",
        "experiment_type": "controlled_experiment"
    });
    
    let vector = encoder.encode(&conditions, &registry).unwrap();
    
    // Each categorical subvector should be normalized to unit length
    let location_start = 0;
    let location_end = location_start + 4;
    let location_subvector = &vector[location_start..location_end];
    let location_norm: f32 = location_subvector.iter().map(|&x| x * x).sum::<f32>().sqrt();
    assert!((location_norm - 1.0).abs() < 0.001, "Location subvector should be normalized");
    
    let experiment_start = 4;
    let experiment_end = experiment_start + 4;
    let experiment_subvector = &vector[experiment_start..experiment_end];
    let experiment_norm: f32 = experiment_subvector.iter().map(|&x| x * x).sum::<f32>().sqrt();
    assert!((experiment_norm - 1.0).abs() < 0.001, "Experiment subvector should be normalized");
}

#[tokio::test]
async fn test_capacity_exclusion_behavior() {
    let encoder = StructuredVectorEncoder::new(
        6,  // reduced reserved_dims
        2,  // dims_per_numeric_key  
        4,  // dims_per_categorical_key
    );
    
    let registry = create_test_registry();
    
    // This should exceed capacity: temperature (2) + location (4) + confidence (2) = 8 dims > 6
    let conditions = json!({
        "temperature": 25.0,
        "location": "lab_a", 
        "confidence": 0.95
    });
    
    let vector = encoder.encode(&conditions, &registry).unwrap();
    
    assert_eq!(vector.len(), 6);
    
    // Should handle capacity gracefully - some keys excluded
    // The exact behavior depends on implementation priority
}

#[tokio::test]
async fn test_vector_bounds() {
    let encoder = create_test_encoder();
    let registry = create_test_registry();
    
    let conditions = json!({
        "temperature": 25.0,
        "location": "lab_a"
    });
    
    let vector = encoder.encode(&conditions, &registry).unwrap();
    
    // All values should be within reasonable bounds
    for &value in &vector {
        assert!(value.is_finite(), "All values should be finite");
        assert!(value >= -10.0 && value <= 10.0, "Values should be within reasonable bounds");
    }
}

#[tokio::test]
async fn test_sparse_conditions() {
    let encoder = create_test_encoder();
    let registry = create_test_registry();
    
    // Test with only one key out of many registered
    let conditions = json!({
        "temperature": 25.0
        // pressure, location, experiment_type, confidence, device_id all missing
    });
    
    let vector = encoder.encode(&conditions, &registry).unwrap();
    
    assert_eq!(vector.len(), 10);
    
    // Should have non-zero values only for temperature (2 dims)
    let non_zero_count = vector.iter().filter(|&&x| x != 0.0).count();
    assert!(non_zero_count <= 2, "Should only have non-zero values for temperature");
}

#[tokio::test]
async fn test_encoder_state_consistency() {
    let encoder = create_test_encoder();
    let registry = create_test_registry();
    
    // Multiple encodes should not affect encoder state
    let conditions1 = json!({"temperature": 25.0});
    let conditions2 = json!({"location": "lab_a"});
    
    let vector1 = encoder.encode(&conditions1, &registry).unwrap();
    let vector2 = encoder.encode(&conditions2, &registry).unwrap();
    let vector1_again = encoder.encode(&conditions1, &registry).unwrap();
    
    assert_eq!(vector1, vector1_again, "Encoder state should be consistent across calls");
    assert_ne!(vector1, vector2, "Different conditions should produce different vectors");
}

#[tokio::test]
async fn test_special_categorical_characters() {
    let encoder = create_test_encoder();
    let registry = create_test_registry();
    
    // Test with special characters, unicode, etc.
    let conditions = json!({
        "location": "lab_α-βγ_测试",
        "experiment_type": "controlled-experiment_v2.0"
    });
    
    let vector = encoder.encode(&conditions, &registry).unwrap();
    
    assert_eq!(vector.len(), 10);
    assert!(vector.iter().any(|&x| x != 0.0), "Should handle special characters");
    
    // Should be deterministic
    let vector2 = encoder.encode(&conditions, &registry).unwrap();
    assert_eq!(vector, vector2, "Special characters should be handled deterministically");
}
