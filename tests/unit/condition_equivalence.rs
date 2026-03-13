//! Unit tests for condition equivalence logic
//! 
//! Tests the ConditionRegistry::is_equivalent method which determines
//! if two condition objects are semantically equivalent for the purposes
//! of atom comparison and graph traversal.

use serde_json::json;
use mote::domain::condition::{ConditionRegistry, ValueType, ConditionValue, ConditionOperator};

fn create_test_registry() -> ConditionRegistry {
    let mut registry = ConditionRegistry::new();
    
    // Register test conditions
    registry.register("temperature".to_string(), ValueType::Numeric, "Temperature in Celsius".to_string());
    registry.register("pressure".to_string(), ValueType::Numeric, "Pressure in kPa".to_string());
    registry.register("location".to_string(), ValueType::Categorical, "Location identifier".to_string());
    registry.register("experiment_type".to_string(), ValueType::Categorical, "Type of experiment".to_string());
    registry.register("confidence".to_string(), ValueType::Numeric, "Confidence score".to_string());
    
    registry
}

#[tokio::test]
async fn test_numeric_exact_match() {
    let registry = create_test_registry();
    
    let conditions1 = json!({
        "temperature": 25.0,
        "pressure": 101.3
    });
    
    let conditions2 = json!({
        "temperature": 25.0,
        "pressure": 101.3
    });
    
    assert!(registry.is_equivalent(&conditions1, &conditions2, 0.01));
}

#[tokio::test]
async fn test_numeric_within_tolerance() {
    let registry = create_test_registry();
    
    let conditions1 = json!({
        "temperature": 25.0,
        "pressure": 101.3
    });
    
    let conditions2 = json!({
        "temperature": 25.005, // Within 0.01 tolerance
        "pressure": 101.305  // Within 0.01 tolerance
    });
    
    assert!(registry.is_equivalent(&conditions1, &conditions2, 0.01));
}

#[tokio::test]
async fn test_numeric_outside_tolerance() {
    let registry = create_test_registry();
    
    let conditions1 = json!({
        "temperature": 25.0,
        "pressure": 101.3
    });
    
    let conditions2 = json!({
        "temperature": 25.02, // Outside 0.01 tolerance
        "pressure": 101.3
    });
    
    assert!(!registry.is_equivalent(&conditions1, &conditions2, 0.01));
}

#[tokio::test]
async fn test_string_exact_match() {
    let registry = create_test_registry();
    
    let conditions1 = json!({
        "location": "lab_a",
        "experiment_type": "controlled"
    });
    
    let conditions2 = json!({
        "location": "lab_a",
        "experiment_type": "controlled"
    });
    
    assert!(registry.is_equivalent(&conditions1, &conditions2, 0.01));
}

#[tokio::test]
async fn test_string_case_sensitive() {
    let registry = create_test_registry();
    
    let conditions1 = json!({
        "location": "lab_a"
    });
    
    let conditions2 = json!({
        "location": "Lab_A" // Different case
    });
    
    assert!(!registry.is_equivalent(&conditions1, &conditions2, 0.01));
}

#[tokio::test]
async fn test_missing_required_key() {
    let registry = create_test_registry();
    
    let conditions1 = json!({
        "temperature": 25.0,
        "pressure": 101.3
    });
    
    let conditions2 = json!({
        "temperature": 25.0
        // Missing "pressure" key
    });
    
    assert!(!registry.is_equivalent(&conditions1, &conditions2, 0.01));
}

#[tokio::test]
async fn test_extra_key_ignored() {
    let registry = create_test_registry();
    
    let conditions1 = json!({
        "temperature": 25.0,
        "pressure": 101.3
    });
    
    let conditions2 = json!({
        "temperature": 25.0,
        "pressure": 101.3,
        "extra_field": "ignored" // Extra field should be ignored
    });
    
    assert!(registry.is_equivalent(&conditions1, &conditions2, 0.01));
}

#[tokio::test]
async fn test_value_type_mismatch() {
    let registry = create_test_registry();
    
    let conditions1 = json!({
        "temperature": 25.0 // Numeric
    });
    
    let conditions2 = json!({
        "temperature": "25.0" // String - type mismatch
    });
    
    assert!(!registry.is_equivalent(&conditions1, &conditions2, 0.01));
}

#[tokio::test]
async fn test_empty_conditions() {
    let registry = create_test_registry();
    
    let conditions1 = json!({});
    let conditions2 = json!({});
    
    assert!(registry.is_equivalent(&conditions1, &conditions2, 0.01));
}

#[tokio::test]
async fn test_empty_vs_non_empty() {
    let registry = create_test_registry();
    
    let conditions1 = json!({});
    let conditions2 = json!({
        "temperature": 25.0
    });
    
    assert!(!registry.is_equivalent(&conditions1, &conditions2, 0.01));
}

#[tokio::test]
async fn test_large_numbers_tolerance() {
    let registry = create_test_registry();
    
    let conditions1 = json!({
        "temperature": 1000000.0,
        "pressure": 2000000.0
    });
    
    let conditions2 = json!({
        "temperature": 1000000.5, // Small relative difference
        "pressure": 2000000.5
    });
    
    // With small tolerance, should be equivalent
    assert!(registry.is_equivalent(&conditions1, &conditions2, 1.0));
    
    // With very small tolerance, should not be equivalent
    assert!(!registry.is_equivalent(&conditions1, &conditions2, 0.1));
}

#[tokio::test]
async fn test_very_small_numbers_tolerance() {
    let registry = create_test_registry();
    
    let conditions1 = json!({
        "temperature": 0.001,
        "pressure": 0.002
    });
    
    let conditions2 = json!({
        "temperature": 0.001001,
        "pressure": 0.002001
    });
    
    // Should handle small numbers correctly
    assert!(registry.is_equivalent(&conditions1, &conditions2, 0.00001));
}

#[tokio::test]
async fn test_nested_conditions() {
    let registry = create_test_registry();
    
    let conditions1 = json!({
        "temperature": 25.0,
        "metadata": {
            "nested": "value",
            "ignored": "data"
        }
    });
    
    let conditions2 = json!({
        "temperature": 25.0,
        "metadata": {
            "nested": "value",
            "different": "ignored" // Different nested field should be ignored
        }
    });
    
    assert!(registry.is_equivalent(&conditions1, &conditions2, 0.01));
}

#[tokio::test]
async fn test_serialization_order_independence() {
    let registry = create_test_registry();
    
    // Create conditions with different key order
    let conditions1_str = r#"{"temperature":25.0,"pressure":101.3,"location":"lab_a"}"#;
    let conditions2_str = r#"{"location":"lab_a","pressure":101.3,"temperature":25.0}"#;
    
    let conditions1: serde_json::Value = serde_json::from_str(conditions1_str).unwrap();
    let conditions2: serde_json::Value = serde_json::from_str(conditions2_str).unwrap();
    
    assert!(registry.is_equivalent(&conditions1, &conditions2, 0.01));
}

#[tokio::test]
async fn test_null_values() {
    let registry = create_test_registry();
    
    let conditions1 = json!({
        "temperature": 25.0,
        "pressure": null
    });
    
    let conditions2 = json!({
        "temperature": 25.0,
        "pressure": null
    });
    
    assert!(registry.is_equivalent(&conditions1, &conditions2, 0.01));
}

#[tokio::test]
async fn test_null_vs_missing() {
    let registry = create_test_registry();
    
    let conditions1 = json!({
        "temperature": 25.0,
        "pressure": null
    });
    
    let conditions2 = json!({
        "temperature": 25.0
        // pressure key missing entirely
    });
    
    // null vs missing should be considered different
    assert!(!registry.is_equivalent(&conditions1, &conditions2, 0.01));
}

#[tokio::test]
async fn test_zero_tolerance() {
    let registry = create_test_registry();
    
    let conditions1 = json!({
        "temperature": 25.0,
        "pressure": 101.3
    });
    
    let conditions2 = json!({
        "temperature": 25.0,
        "pressure": 101.3000001 // Slight difference
    });
    
    // With zero tolerance, exact match required
    assert!(!registry.is_equivalent(&conditions1, &conditions2, 0.0));
}

#[tokio::test]
async fn test_large_tolerance() {
    let registry = create_test_registry();
    
    let conditions1 = json!({
        "temperature": 25.0,
        "pressure": 101.3
    });
    
    let conditions2 = json!({
        "temperature": 30.0, // Large difference
        "pressure": 150.0  // Large difference
    });
    
    // With large tolerance, should be equivalent
    assert!(registry.is_equivalent(&conditions1, &conditions2, 100.0));
}

#[tokio::test]
async fn test_mixed_types() {
    let registry = create_test_registry();
    
    let conditions1 = json!({
        "temperature": 25.0,
        "location": "lab_a",
        "confidence": 0.95
    });
    
    let conditions2 = json!({
        "temperature": 25.001,
        "location": "lab_a",
        "confidence": 0.951
    });
    
    assert!(registry.is_equivalent(&conditions1, &conditions2, 0.01));
}

#[tokio::test]
async fn test_unregistered_keys_ignored() {
    let registry = create_test_registry();
    
    let conditions1 = json!({
        "temperature": 25.0,
        "unregistered_field": "value1"
    });
    
    let conditions2 = json!({
        "temperature": 25.0,
        "unregistered_field": "value2" // Different value but unregistered
    });
    
    // Should be equivalent since unregistered fields are ignored
    assert!(registry.is_equivalent(&conditions1, &conditions2, 0.01));
}

#[tokio::test]
async fn test_array_values_not_equivalent() {
    let registry = create_test_registry();
    
    let conditions1 = json!({
        "temperature": [25.0, 26.0] // Array instead of scalar
    });
    
    let conditions2 = json!({
        "temperature": 25.0
    });
    
    // Arrays should not be equivalent to scalars
    assert!(!registry.is_equivalent(&conditions1, &conditions2, 0.01));
}
