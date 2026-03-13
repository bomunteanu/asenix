//! Unit tests for atom ID hashing determinism
//! 
//! Tests the compute_atom_id function to ensure:
//! - Same inputs always produce identical IDs
//! - Changing any required field changes the ID
//! - All required fields participate in the hash
//! - Provenance and conditions are properly included

use serde_json::json;
use mote::domain::atom::{AtomType, Provenance};
use mote::crypto::hashing::compute_atom_id;

fn create_test_provenance() -> Provenance {
    Provenance {
        methodology: "controlled_experiment".to_string(),
        data_source: "lab_measurement".to_string(),
        confidence: 0.95,
        parent_ids: vec!["parent_1".to_string(), "parent_2".to_string()],
        metadata: json!({
            "equipment": "spectrometer_v2",
            "calibration": "certified"
        }),
    }
}

fn create_base_atom_input() -> (AtomType, String, String, serde_json::Value, Provenance) {
    (
        AtomType::Finding,
        "physics".to_string(),
        "Temperature measurement shows 25.3°C with 0.1°C uncertainty".to_string(),
        json!({
            "temperature": 25.3,
            "uncertainty": 0.1,
            "units": "celsius",
            "location": "lab_a"
        }),
        create_test_provenance(),
    )
}

#[tokio::test]
async fn test_deterministic_same_inputs() {
    let (atom_type, domain, statement, conditions, provenance) = create_base_atom_input();
    
    // Compute ID twice with same inputs
    let id1 = compute_atom_id(&atom_type, &domain, &statement, &conditions, &provenance, 1640995200);
    let id2 = compute_atom_id(&atom_type, &domain, &statement, &conditions, &provenance, 1640995200);
    
    assert_eq!(id1, id2, "Same inputs should produce identical IDs");
    
    // Verify ID is non-empty and reasonable length
    assert!(!id1.is_empty());
    assert!(id1.len() > 10); // Blake3 hash should be substantial
}

#[tokio::test]
async fn test_atom_type_changes_id() {
    let (atom_type, domain, statement, conditions, provenance) = create_base_atom_input();
    
    let original_id = compute_atom_id(&atom_type, &domain, &statement, &conditions, &provenance, 1640995200);
    
    // Change atom type
    let different_type = AtomType::Hypothesis;
    let changed_id = compute_atom_id(&different_type, &domain, &statement, &conditions, &provenance, 1640995200);
    
    assert_ne!(original_id, changed_id, "Changing atom type should change the ID");
}

#[tokio::test]
async fn test_domain_changes_id() {
    let (atom_type, domain, statement, conditions, provenance) = create_base_atom_input();
    
    let original_id = compute_atom_id(&atom_type, &domain, &statement, &conditions, &provenance, 1640995200);
    
    // Change domain
    let different_domain = "chemistry".to_string();
    let changed_id = compute_atom_id(&atom_type, &different_domain, &statement, &conditions, &provenance, 1640995200);
    
    assert_ne!(original_id, changed_id, "Changing domain should change the ID");
}

#[tokio::test]
async fn test_statement_changes_id() {
    let (atom_type, domain, statement, conditions, provenance) = create_base_atom_input();
    
    let original_id = compute_atom_id(&atom_type, &domain, &statement, &conditions, &provenance, 1640995200);
    
    // Change statement (even slightly)
    let different_statement = "Temperature measurement shows 25.3°C with 0.1°C uncertainty ".to_string(); // Extra space
    let changed_id = compute_atom_id(&atom_type, &domain, &different_statement, &conditions, &provenance, 1640995200);
    
    assert_ne!(original_id, changed_id, "Changing statement should change the ID");
}

#[tokio::test]
async fn test_conditions_changes_id() {
    let (atom_type, domain, statement, conditions, provenance) = create_base_atom_input();
    
    let original_id = compute_atom_id(&atom_type, &domain, &statement, &conditions, &provenance, 1640995200);
    
    // Change conditions slightly
    let different_conditions = json!({
        "temperature": 25.3,
        "uncertainty": 0.1,
        "units": "celsius",
        "location": "lab_a",
        "extra_field": "added" // Extra field
    });
    
    let changed_id = compute_atom_id(&atom_type, &domain, &statement, &different_conditions, &provenance, 1640995200);
    
    assert_ne!(original_id, changed_id, "Changing conditions should change the ID");
}

#[tokio::test]
async fn test_provenance_changes_id() {
    let (atom_type, domain, statement, conditions, provenance) = create_base_atom_input();
    
    let original_id = compute_atom_id(&atom_type, &domain, &statement, &conditions, &provenance, 1640995200);
    
    // Change provenance methodology
    let mut different_provenance = provenance.clone();
    different_provenance.methodology = "observational_study".to_string();
    
    let changed_id = compute_atom_id(&atom_type, &domain, &statement, &conditions, &different_provenance, 1640995200);
    
    assert_ne!(original_id, changed_id, "Changing provenance should change the ID");
}

#[tokio::test]
async fn test_timestamp_changes_id() {
    let (atom_type, domain, statement, conditions, provenance) = create_base_atom_input();
    
    let original_id = compute_atom_id(&atom_type, &domain, &statement, &conditions, &provenance, 1640995200);
    
    // Change timestamp by 1 second
    let changed_id = compute_atom_id(&atom_type, &domain, &statement, &conditions, &provenance, 1640995201);
    
    assert_ne!(original_id, changed_id, "Changing timestamp should change the ID");
}

#[tokio::test]
async fn test_provenance_parent_ids_change_id() {
    let (atom_type, domain, statement, conditions, provenance) = create_base_atom_input();
    
    let original_id = compute_atom_id(&atom_type, &domain, &statement, &conditions, &provenance, 1640995200);
    
    // Change parent IDs order
    let mut different_provenance = provenance.clone();
    different_provenance.parent_ids = vec!["parent_2".to_string(), "parent_1".to_string()]; // Swapped order
    
    let changed_id = compute_atom_id(&atom_type, &domain, &statement, &conditions, &different_provenance, 1640995200);
    
    assert_ne!(original_id, changed_id, "Changing parent IDs order should change the ID");
}

#[tokio::test]
async fn test_provenance_metadata_changes_id() {
    let (atom_type, domain, statement, conditions, provenance) = create_base_atom_input();
    
    let original_id = compute_atom_id(&atom_type, &domain, &statement, &conditions, &provenance, 1640995200);
    
    // Change metadata
    let mut different_provenance = provenance.clone();
    different_provenance.metadata = json!({
        "equipment": "spectrometer_v3", // Different version
        "calibration": "certified"
    });
    
    let changed_id = compute_atom_id(&atom_type, &domain, &statement, &conditions, &different_provenance, 1640995200);
    
    assert_ne!(original_id, changed_id, "Changing provenance metadata should change the ID");
}

#[tokio::test]
async fn test_all_atom_types_produce_different_ids() {
    let domain = "physics".to_string();
    let statement = "Test statement".to_string();
    let conditions = json!({"test": "value"});
    let provenance = create_test_provenance();
    
    let mut ids = std::collections::HashSet::new();
    
    for atom_type in [
        AtomType::Hypothesis,
        AtomType::Finding,
        AtomType::NegativeResult,
        AtomType::Delta,
        AtomType::ExperimentLog,
        AtomType::Synthesis,
        AtomType::Bounty,
    ] {
        let id = compute_atom_id(&atom_type, &domain, &statement, &conditions, &provenance, 1640995200);
        assert!(!ids.contains(&id), "Atom type {:?} produced duplicate ID", atom_type);
        ids.insert(id);
    }
    
    assert_eq!(ids.len(), 7, "All atom types should produce unique IDs");
}

#[tokio::test]
async fn test_empty_conditions_vs_null_conditions() {
    let (atom_type, domain, statement, _, provenance) = create_base_atom_input();
    
    // Empty object vs null should produce different IDs
    let empty_conditions = json!({});
    let null_conditions = json!(null);
    
    let id_empty = compute_atom_id(&atom_type, &domain, &statement, &empty_conditions, &provenance, 1640995200);
    let id_null = compute_atom_id(&atom_type, &domain, &statement, &null_conditions, &provenance, 1640995200);
    
    assert_ne!(id_empty, id_null, "Empty conditions vs null conditions should produce different IDs");
}

#[tokio::test]
async fn test_complex_nested_conditions() {
    let (atom_type, domain, statement, _, provenance) = create_base_atom_input();
    
    let complex_conditions = json!({
        "measurements": [
            {"value": 25.3, "unit": "celsius"},
            {"value": 101.3, "unit": "kpa"}
        ],
        "metadata": {
            "experiment": {
                "id": "exp_123",
                "run": 5,
                "parameters": {
                    "duration": 3600,
                    "precision": 0.01
                }
            }
        },
        "tags": ["temperature", "pressure", "controlled"]
    });
    
    let id1 = compute_atom_id(&atom_type, &domain, &statement, &complex_conditions, &provenance, 1640995200);
    let id2 = compute_atom_id(&atom_type, &domain, &statement, &complex_conditions, &provenance, 1640995200);
    
    assert_eq!(id1, id2, "Complex nested conditions should be deterministic");
    
    // Change a nested value
    let mut modified_conditions = complex_conditions.clone();
    modified_conditions["metadata"]["experiment"]["run"] = json!(6);
    
    let id_modified = compute_atom_id(&atom_type, &domain, &statement, &modified_conditions, &provenance, 1640995200);
    
    assert_ne!(id1, id_modified, "Changing nested condition should change ID");
}

#[tokio::test]
async fn test_unicode_and_special_characters() {
    let (atom_type, _, statement, conditions, provenance) = create_base_atom_input();
    
    // Test with unicode characters
    let unicode_domain = "物理实验".to_string(); // Chinese for "physics experiment"
    let unicode_statement = "温度测量显示 25.3°C".to_string();
    let unicode_conditions = json!({
        "location": "实验室A",
        "notes": "测试数据 ✓ ✓ ✓"
    });
    
    let id1 = compute_atom_id(&atom_type, &unicode_domain, &unicode_statement, &unicode_conditions, &provenance, 1640995200);
    let id2 = compute_atom_id(&atom_type, &unicode_domain, &unicode_statement, &unicode_conditions, &provenance, 1640995200);
    
    assert_eq!(id1, id2, "Unicode characters should be handled deterministically");
    
    // Verify it's different from ASCII version
    let ascii_id = compute_atom_id(&atom_type, &"physics".to_string(), &"Temperature measurement".to_string(), &conditions, &provenance, 1640995200);
    
    assert_ne!(id1, ascii_id, "Unicode should produce different ID than ASCII");
}

#[tokio::test]
async fn test_all_required_fields_documented() {
    // This test explicitly documents which fields are included in the hash
    let (atom_type, domain, statement, conditions, provenance) = create_base_atom_input();
    
    let base_id = compute_atom_id(&atom_type, &domain, &statement, &conditions, &provenance, 1640995200);
    
    // Test each field individually to ensure they all participate
    let tests = [
        ("atom_type", {
            let id = compute_atom_id(&AtomType::Hypothesis, &domain, &statement, &conditions, &provenance, 1640995200);
            assert_ne!(id, base_id, "atom_type should be included in hash");
        }),
        ("domain", {
            let id = compute_atom_id(&atom_type, &"different", &statement, &conditions, &provenance, 1640995200);
            assert_ne!(id, base_id, "domain should be included in hash");
        }),
        ("statement", {
            let id = compute_atom_id(&atom_type, &domain, &"different", &conditions, &provenance, 1640995200);
            assert_ne!(id, base_id, "statement should be included in hash");
        }),
        ("conditions", {
            let id = compute_atom_id(&atom_type, &domain, &statement, &json!({"different": "value"}), &provenance, 1640995200);
            assert_ne!(id, base_id, "conditions should be included in hash");
        }),
        ("provenance.methodology", {
            let mut p = provenance.clone();
            p.methodology = "different".to_string();
            let id = compute_atom_id(&atom_type, &domain, &statement, &conditions, &p, 1640995200);
            assert_ne!(id, base_id, "provenance.methodology should be included in hash");
        }),
        ("provenance.data_source", {
            let mut p = provenance.clone();
            p.data_source = "different".to_string();
            let id = compute_atom_id(&atom_type, &domain, &statement, &conditions, &p, 1640995200);
            assert_ne!(id, base_id, "provenance.data_source should be included in hash");
        }),
        ("provenance.confidence", {
            let mut p = provenance.clone();
            p.confidence = 0.94;
            let id = compute_atom_id(&atom_type, &domain, &statement, &conditions, &p, 1640995200);
            assert_ne!(id, base_id, "provenance.confidence should be included in hash");
        }),
        ("provenance.parent_ids", {
            let mut p = provenance.clone();
            p.parent_ids = vec!["different".to_string()];
            let id = compute_atom_id(&atom_type, &domain, &statement, &conditions, &p, 1640995200);
            assert_ne!(id, base_id, "provenance.parent_ids should be included in hash");
        }),
        ("provenance.metadata", {
            let mut p = provenance.clone();
            p.metadata = json!({"different": "value"});
            let id = compute_atom_id(&atom_type, &domain, &statement, &conditions, &p, 1640995200);
            assert_ne!(id, base_id, "provenance.metadata should be included in hash");
        }),
        ("timestamp", {
            let id = compute_atom_id(&atom_type, &domain, &statement, &conditions, &provenance, 1640995201);
            assert_ne!(id, base_id, "timestamp should be included in hash");
        }),
    ];
    
    // All tests should pass
    for (field_name, _) in tests {
        // Test already executed above
    }
}

#[tokio::test]
async fn test_hash_consistency_across_serialization_formats() {
    let (atom_type, domain, statement, conditions, provenance) = create_base_atom_input();
    
    // Create conditions with different JSON formatting but same logical content
    let conditions_formatted1 = json!({
        "temperature": 25.3,
        "uncertainty": 0.1,
        "location": "lab_a"
    });
    
    let conditions_formatted2 = json!({
        "location": "lab_a",
        "temperature": 25.3,
        "uncertainty": 0.1
    });
    
    let id1 = compute_atom_id(&atom_type, &domain, &statement, &conditions_formatted1, &provenance, 1640995200);
    let id2 = compute_atom_id(&atom_type, &domain, &statement, &conditions_formatted2, &provenance, 1640995200);
    
    // These should be different because JSON serialization order affects the hash
    // This is expected behavior - canonical JSON serialization would be needed for order independence
    assert_ne!(id1, id2, "Different JSON serialization order should produce different IDs (expected behavior)");
}

#[tokio::test]
async fn test_edge_cases() {
    let (atom_type, domain, statement, _, provenance) = create_base_atom_input();
    
    // Test with very long strings
    let long_statement = "A".repeat(10000);
    let long_id = compute_atom_id(&atom_type, &domain, &long_statement, &json!({}), &provenance, 1640995200);
    assert!(!long_id.is_empty(), "Very long statements should be handled");
    
    // Test with empty strings
    let empty_id = compute_atom_id(&atom_type, &"".to_string(), &"".to_string(), &json!({}), &provenance, 1640995200);
    assert!(!empty_id.is_empty(), "Empty strings should be handled");
    
    // Test with maximum timestamp
    let max_id = compute_atom_id(&atom_type, &domain, &statement, &json!({}), &provenance, u64::MAX);
    assert!(!max_id.is_empty(), "Maximum timestamp should be handled");
}
