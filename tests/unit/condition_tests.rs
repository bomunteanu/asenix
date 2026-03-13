use mote::domain::condition::{ConditionRegistry, ValueType, ConditionOperator};
use serde_json::json;

#[test]
fn test_condition_equivalence_numeric_values() {
    let registry = ConditionRegistry {
        domain: "physics".to_string(),
        key_name: "temperature".to_string(),
        value_type: ValueType::Float,
        unit: Some("celsius".to_string()),
        required: true,
    };

    let conditions1 = json!({
        "temperature": 25.0,
        "pressure": 101.3
    });

    let conditions2 = json!({
        "temperature": 25.0,
        "pressure": 101.3
    });

    assert!(registry.is_equivalent(&conditions1, &conditions2, 0.01),
        "Identical numeric values should be equivalent");

    // Test within tolerance
    let conditions3 = json!({
        "temperature": 25.005,
        "pressure": 101.3
    });

    assert!(registry.is_equivalent(&conditions1, &conditions3, 0.01),
        "Values within tolerance should be equivalent");

    // Test outside tolerance
    let conditions4 = json!({
        "temperature": 25.02,
        "pressure": 101.3
    });

    assert!(!registry.is_equivalent(&conditions1, &conditions4, 0.01),
        "Values outside tolerance should not be equivalent");
}

#[test]
fn test_condition_equivalence_string_values() {
    let registry = ConditionRegistry {
        domain: "chemistry".to_string(),
        key_name: "experiment_type".to_string(),
        value_type: ValueType::String,
        unit: None,
        required: true,
    };

    let conditions1 = json!({
        "experiment_type": "titration",
        "reagent": "HCl"
    });

    let conditions2 = json!({
        "experiment_type": "titration",
        "reagent": "HCl"
    });

    assert!(registry.is_equivalent(&conditions1, &conditions2, 0.01),
        "Identical string values should be equivalent");

    let conditions3 = json!({
        "experiment_type": "distillation",
        "reagent": "HCl"
    });

    assert!(!registry.is_equivalent(&conditions1, &conditions3, 0.01),
        "Different string values should not be equivalent");
}

#[test]
fn test_condition_equivalence_integer_values() {
    let registry = ConditionRegistry {
        domain: "biology".to_string(),
        key_name: "sample_count".to_string(),
        value_type: ValueType::Int,
        unit: Some("samples".to_string()),
        required: true,
    };

    let conditions1 = json!({
        "sample_count": 42,
        "species": "E. coli"
    });

    let conditions2 = json!({
        "sample_count": 42,
        "species": "E. coli"
    });

    assert!(registry.is_equivalent(&conditions1, &conditions2, 0.01),
        "Identical integer values should be equivalent");

    let conditions3 = json!({
        "sample_count": 43,
        "species": "E. coli"
    });

    assert!(!registry.is_equivalent(&conditions1, &conditions3, 0.01),
        "Different integer values should not be equivalent");
}

#[test]
fn test_condition_equivalence_optional_key() {
    let registry = ConditionRegistry {
        domain: "physics".to_string(),
        key_name: "optional_param".to_string(),
        value_type: ValueType::Float,
        unit: None,
        required: false,
    };

    let conditions1 = json!({
        "temperature": 25.0
    });

    let conditions2 = json!({
        "temperature": 25.0,
        "optional_param": 1.5
    });

    assert!(registry.is_equivalent(&conditions1, &conditions2, 0.01),
        "Optional key presence should not affect equivalence");

    let conditions3 = json!({
        "temperature": 25.0,
        "optional_param": null
    });

    assert!(registry.is_equivalent(&conditions1, &conditions3, 0.01),
        "Optional key as null should not affect equivalence");
}

#[test]
fn test_condition_equivalence_missing_required_key() {
    let registry = ConditionRegistry {
        domain: "chemistry".to_string(),
        key_name: "concentration".to_string(),
        value_type: ValueType::Float,
        unit: Some("M".to_string()),
        required: true,
    };

    let conditions1 = json!({
        "temperature": 25.0,
        "concentration": 0.1
    });

    let conditions2 = json!({
        "temperature": 25.0
        // Missing concentration
    });

    assert!(!registry.is_equivalent(&conditions1, &conditions2, 0.01),
        "Missing required key should not be equivalent");

    let conditions3 = json!({
        "temperature": 25.0,
        "concentration": null
    });

    assert!(!registry.is_equivalent(&conditions1, &conditions3, 0.01),
        "Null required key should not be equivalent");
}

#[test]
fn test_condition_equivalence_both_missing() {
    let registry = ConditionRegistry {
        domain: "physics".to_string(),
        key_name: "optional_field".to_string(),
        value_type: ValueType::String,
        unit: None,
        required: false,
    };

    let conditions1 = json!({
        "temperature": 25.0
    });

    let conditions2 = json!({
        "temperature": 25.0
    });

    assert!(registry.is_equivalent(&conditions1, &conditions2, 0.01),
        "Both missing optional key should be equivalent");
}

#[test]
fn test_condition_validation() {
    // Test Float validation
    let float_registry = ConditionRegistry {
        domain: "physics".to_string(),
        key_name: "temperature".to_string(),
        value_type: ValueType::Float,
        unit: Some("celsius".to_string()),
        required: true,
    };

    assert!(float_registry.validate_value(&json!(25.5)),
        "Valid float should pass validation");
    assert!(float_registry.validate_value(&json!(25)),
        "Integer should pass float validation");
    assert!(!float_registry.validate_value(&json!("25.5")),
        "String should not pass float validation");
    assert!(!float_registry.validate_value(&json!(null)),
        "Null should not pass float validation");

    // Test Int validation
    let int_registry = ConditionRegistry {
        domain: "biology".to_string(),
        key_name: "sample_count".to_string(),
        value_type: ValueType::Int,
        unit: Some("samples".to_string()),
        required: true,
    };

    assert!(int_registry.validate_value(&json!(42)),
        "Valid integer should pass validation");
    assert!(!int_registry.validate_value(&json!(42.5)),
        "Float should not pass integer validation");
    assert!(!int_registry.validate_value(&json!("42")),
        "String should not pass integer validation");

    // Test String validation
    let string_registry = ConditionRegistry {
        domain: "chemistry".to_string(),
        key_name: "experiment_type".to_string(),
        value_type: ValueType::String,
        unit: None,
        required: true,
    };

    assert!(string_registry.validate_value(&json!("titration")),
        "Valid string should pass validation");
    assert!(!string_registry.validate_value(&json!(123)),
        "Number should not pass string validation");
    assert!(!string_registry.validate_value(&json!(null)),
        "Null should not pass string validation");

    // Test Enum validation (same as String)
    let enum_registry = ConditionRegistry {
        domain: "physics".to_string(),
        key_name: "phase".to_string(),
        value_type: ValueType::Enum,
        unit: None,
        required: true,
    };

    assert!(enum_registry.validate_value(&json!("solid")),
        "Valid enum string should pass validation");
    assert!(!enum_registry.validate_value(&json!(123)),
        "Number should not pass enum validation");
}

#[test]
fn test_condition_operators() {
    // Test that all operators can be created
    let operators = vec![
        ConditionOperator::Equals,
        ConditionOperator::NotEquals,
        ConditionOperator::GreaterThan,
        ConditionOperator::LessThan,
        ConditionOperator::GreaterThanOrEqual,
        ConditionOperator::LessThanOrEqual,
        ConditionOperator::Contains,
        ConditionOperator::NotContains,
    ];

    assert_eq!(operators.len(), 8, "Should have 8 different operators");
}

#[test]
fn test_condition_complex_scenarios() {
    let registry = ConditionRegistry {
        domain: "physics".to_string(),
        key_name: "measurement_error".to_string(),
        value_type: ValueType::Float,
        unit: Some("%".to_string()),
        required: true,
    };

    // Test with very small tolerance
    let conditions1 = json!({
        "measurement_error": 0.001,
        "other_field": "value"
    });

    let conditions2 = json!({
        "measurement_error": 0.0011,  // Difference of 0.0001
        "other_field": "value"
    });

    assert!(!registry.is_equivalent(&conditions1, &conditions2, 0.00005),
        "Very small differences should be detected");

    assert!(registry.is_equivalent(&conditions1, &conditions2, 0.0002),
        "Larger tolerance should allow small differences");

    // Test with extreme values
    let conditions3 = json!({
        "measurement_error": 1e10,
        "other_field": "value"
    });

    let conditions4 = json!({
        "measurement_error": 1e10 + 1.0,
        "other_field": "value"
    });

    assert!(registry.is_equivalent(&conditions3, &conditions4, 2.0),
        "Large numbers should work with appropriate tolerance");
}
