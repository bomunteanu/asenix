use chrono::Utc;
use mote::crypto::hashing::compute_atom_id;
use mote::domain::atom::AtomType;
use serde_json::json;

#[test]
fn test_atom_id_deterministic() {
    let timestamp = Utc::now();
    let conditions = json!({
        "temperature": 25.0,
        "pressure": 101.3,
        "experiment_id": "exp_123"
    });
    let provenance = json!({
        "parent_ids": ["atom_1", "atom_2"],
        "code_hash": "abc123",
        "environment": "python-3.9"
    });

    let atom_id1 = compute_atom_id(
        "hypothesis",
        "physics",
        "Temperature affects reaction rate",
        &conditions,
        &provenance,
        &timestamp,
    );

    let atom_id2 = compute_atom_id(
        "hypothesis",
        "physics",
        "Temperature affects reaction rate",
        &conditions,
        &provenance,
        &timestamp,
    );

    assert_eq!(atom_id1, atom_id2, "Atom IDs should be identical for identical inputs");
    assert!(!atom_id1.is_empty(), "Atom ID should not be empty");
    assert_eq!(atom_id1.len(), 64, "Atom ID should be 64 hex characters (32 bytes)");
}

#[test]
fn test_atom_id_changes_with_field_modification() {
    let timestamp = Utc::now();
    let conditions = json!({
        "temperature": 25.0,
        "pressure": 101.3,
        "experiment_id": "exp_123"
    });
    let provenance = json!({
        "parent_ids": ["atom_1", "atom_2"],
        "code_hash": "abc123",
        "environment": "python-3.9"
    });

    let base_id = compute_atom_id(
        "hypothesis",
        "physics",
        "Temperature affects reaction rate",
        &conditions,
        &provenance,
        &timestamp,
    );

    // Change statement
    let modified_statement_id = compute_atom_id(
        "hypothesis",
        "physics",
        "Temperature affects reaction rate significantly",
        &conditions,
        &provenance,
        &timestamp,
    );
    assert_ne!(base_id, modified_statement_id, "Atom ID should change when statement changes");

    // Change domain
    let modified_domain_id = compute_atom_id(
        "hypothesis",
        "chemistry",
        "Temperature affects reaction rate",
        &conditions,
        &provenance,
        &timestamp,
    );
    assert_ne!(base_id, modified_domain_id, "Atom ID should change when domain changes");

    // Change atom type
    let modified_type_id = compute_atom_id(
        "finding",
        "physics",
        "Temperature affects reaction rate",
        &conditions,
        &provenance,
        &timestamp,
    );
    assert_ne!(base_id, modified_type_id, "Atom ID should change when atom type changes");

    // Change conditions
    let modified_conditions = json!({
        "temperature": 26.0,  // Changed from 25.0
        "pressure": 101.3,
        "experiment_id": "exp_123"
    });
    let modified_conditions_id = compute_atom_id(
        "hypothesis",
        "physics",
        "Temperature affects reaction rate",
        &modified_conditions,
        &provenance,
        &timestamp,
    );
    assert_ne!(base_id, modified_conditions_id, "Atom ID should change when conditions change");

    // Change provenance
    let modified_provenance = json!({
        "parent_ids": ["atom_1", "atom_2"],
        "code_hash": "def456",  // Changed from abc123
        "environment": "python-3.9"
    });
    let modified_provenance_id = compute_atom_id(
        "hypothesis",
        "physics",
        "Temperature affects reaction rate",
        &conditions,
        &modified_provenance,
        &timestamp,
    );
    assert_ne!(base_id, modified_provenance_id, "Atom ID should change when provenance changes");
}

#[test]
fn test_atom_id_changes_with_timestamp() {
    let base_timestamp = Utc::now();
    let conditions = json!({
        "temperature": 25.0,
        "pressure": 101.3
    });
    let provenance = json!({
        "parent_ids": ["atom_1"],
        "code_hash": "abc123"
    });

    let base_id = compute_atom_id(
        "hypothesis",
        "physics",
        "Temperature affects reaction rate",
        &conditions,
        &provenance,
        &base_timestamp,
    );

    // Change timestamp by 1 microsecond
    let modified_timestamp = base_timestamp + chrono::Duration::microseconds(1);
    let modified_timestamp_id = compute_atom_id(
        "hypothesis",
        "physics",
        "Temperature affects reaction rate",
        &conditions,
        &provenance,
        &modified_timestamp,
    );

    assert_ne!(base_id, modified_timestamp_id, "Atom ID should change when timestamp changes");
}

#[test]
fn test_atom_id_different_json_ordering() {
    let timestamp = Utc::now();
    
    // Same JSON content but different order in string representation
    let conditions1 = json!({
        "temperature": 25.0,
        "pressure": 101.3,
        "experiment_id": "exp_123"
    });
    
    let conditions2 = json!({
        "experiment_id": "exp_123",
        "pressure": 101.3,
        "temperature": 25.0
    });
    
    let provenance = json!({
        "parent_ids": ["atom_1", "atom_2"],
        "code_hash": "abc123"
    });

    let id1 = compute_atom_id(
        "hypothesis",
        "physics",
        "Temperature affects reaction rate",
        &conditions1,
        &provenance,
        &timestamp,
    );

    let id2 = compute_atom_id(
        "hypothesis",
        "physics",
        "Temperature affects reaction rate",
        &conditions2,
        &provenance,
        &timestamp,
    );

    // Should be the same because serde_json canonicalizes the ordering
    assert_eq!(id1, id2, "Atom ID should be the same regardless of JSON key order");
}

#[test]
fn test_atom_id_empty_vs_null_fields() {
    let timestamp = Utc::now();
    let conditions = json!({});
    let provenance = json!({});

    let empty_id = compute_atom_id(
        "hypothesis",
        "physics",
        "Temperature affects reaction rate",
        &conditions,
        &provenance,
        &timestamp,
    );

    // Test with null values
    let null_conditions = json!({
        "temperature": null,
        "pressure": null
    });
    let null_provenance = json!({
        "parent_ids": null,
        "code_hash": null
    });

    let null_id = compute_atom_id(
        "hypothesis",
        "physics",
        "Temperature affects reaction rate",
        &null_conditions,
        &null_provenance,
        &timestamp,
    );

    // Empty objects and objects with null values should produce different hashes
    assert_ne!(empty_id, null_id, "Empty objects and null objects should produce different atom IDs");
}
