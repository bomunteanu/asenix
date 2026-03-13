//! Working unit tests for condition equivalence
//! 
//! Tests the ConditionRegistry::is_equivalent method with the actual code structure

use serde_json::json;
use mote::domain::condition::{ConditionRegistry, ValueType};

fn create_test_registry() -> Vec<ConditionRegistry> {
    vec![
        ConditionRegistry {
            domain: "physics".to_string(),
            key_name: "temperature".to_string(),
            value_type: ValueType::Float,
            unit: Some("celsius".to_string()),
            required: true,
        },
        ConditionRegistry {
            domain: "physics".to_string(),
            key_name: "pressure".to_string(),
            value_type: ValueType::Float,
            unit: Some("kpa".to_string()),
            required: true,
        },
        ConditionRegistry {
            domain: "physics".to_string(),
            key_name: "location".to_string(),
            value_type: ValueType::String,
            unit: None,
            required: true,
        },
        ConditionRegistry {
            domain: "physics".to_string(),
            key_name: "experiment_type".to_string(),
            value_type: ValueType::Enum,
            unit: None,
            required: false,
        },
    ]
}

#[tokio::test]
async fn test_float_equivalence_within_tolerance() {
    let registry = create_test_registry();
    let temp_registry = &registry[0]; // temperature registry
    
    let conditions1 = json!({
        "temperature": 25.0,
        "pressure": 101.3
    });
    
    let conditions2 = json!({
        "temperature": 25.1,
        "pressure": 101.3
    });
    
    // Should be equivalent within 0.1 tolerance
    assert!(temp_registry.is_equivalent(&conditions1, &conditions2, 0.11));
    
    // Should not be equivalent with 0.05 tolerance
    assert!(!temp_registry.is_equivalent(&conditions1, &conditions2, 0.05));
}

#[tokio::test]
async fn test_string_exact_matching() {
    let registry = create_test_registry();
    let location_registry = &registry[2]; // location registry
    
    let conditions1 = json!({
        "location": "lab_a",
        "temperature": 25.0
    });
    
    let conditions2 = json!({
        "location": "lab_a",
        "temperature": 30.0
    });
    
    // Should be equivalent for exact string match
    assert!(location_registry.is_equivalent(&conditions1, &conditions2, 0.1));
    
    let conditions3 = json!({
        "location": "lab_b",
        "temperature": 25.0
    });
    
    // Should not be equivalent for different string
    assert!(!location_registry.is_equivalent(&conditions1, &conditions3, 0.1));
}

#[tokio::test]
async fn test_missing_keys_both_absent() {
    let registry = create_test_registry();
    let experiment_registry = &registry[3]; // experiment_type registry (optional)
    
    let conditions1 = json!({
        "temperature": 25.0,
        "pressure": 101.3
    });
    
    let conditions2 = json!({
        "temperature": 30.0,
        "pressure": 101.3
    });
    
    // Both missing the optional key should be equivalent
    assert!(experiment_registry.is_equivalent(&conditions1, &conditions2, 0.1));
}

#[tokio::test]
async fn test_missing_keys_one_absent() {
    let registry = create_test_registry();
    let temp_registry = &registry[0]; // temperature registry (required)
    
    let conditions1 = json!({
        "temperature": 25.0,
        "pressure": 101.3
    });
    
    let conditions2 = json!({
        "pressure": 101.3
    });
    
    // Required key missing in one should not be equivalent
    assert!(!temp_registry.is_equivalent(&conditions1, &conditions2, 0.1));
}

#[tokio::test]
async fn test_int_exact_matching() {
    let int_registry = ConditionRegistry {
        domain: "test".to_string(),
        key_name: "count".to_string(),
        value_type: ValueType::Int,
        unit: None,
        required: true,
    };
    
    let conditions1 = json!({
        "count": 42,
        "other": "value"
    });
    
    let conditions2 = json!({
        "count": 42,
        "other": "different"
    });
    
    // Should be equivalent for exact int match
    assert!(int_registry.is_equivalent(&conditions1, &conditions2, 0.1));
    
    let conditions3 = json!({
        "count": 43,
        "other": "value"
    });
    
    // Should not be equivalent for different int
    assert!(!int_registry.is_equivalent(&conditions1, &conditions3, 0.1));
}

#[tokio::test]
async fn test_value_validation() {
    let registry = create_test_registry();
    
    // Test float validation
    let temp_registry = &registry[0];
    assert!(temp_registry.validate_value(&json!(25.0)));
    assert!(temp_registry.validate_value(&json!(-10.5)));
    assert!(!temp_registry.validate_value(&json!("not a number")));
    assert!(!temp_registry.validate_value(&json!(true)));
    
    // Test string validation
    let location_registry = &registry[2];
    assert!(location_registry.validate_value(&json!("lab_a")));
    assert!(location_registry.validate_value(&json!("")));
    assert!(!location_registry.validate_value(&json!(123)));
    assert!(!location_registry.validate_value(&json!(true)));
    
    // Test int validation
    let int_registry = ConditionRegistry {
        domain: "test".to_string(),
        key_name: "count".to_string(),
        value_type: ValueType::Int,
        unit: None,
        required: true,
    };
    assert!(int_registry.validate_value(&json!(42)));
    assert!(int_registry.validate_value(&json!(-10)));
    assert!(!int_registry.validate_value(&json!(42.5))); // Float, not int
    assert!(!int_registry.validate_value(&json!("42"))); // String, not int
}
