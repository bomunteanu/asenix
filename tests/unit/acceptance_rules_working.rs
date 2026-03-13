//! Working unit tests for acceptance rules
//! 
//! Tests the AcceptancePipeline logic with the actual code structure

use serde_json::json;
use mote::domain::atom::{AtomInput, AtomType};

#[tokio::test]
async fn test_acceptance_pipeline_statement_length_validation() {
    let pipeline = mote::acceptance::AcceptancePipeline::new();
    
    // Test statement too short (but provide metrics to avoid atom type rule triggering first)
    let short_atom = AtomInput {
        atom_type: AtomType::Finding,
        domain: "test".to_string(),
        statement: "short".to_string(), // Less than 10 characters
        conditions: json!({}),
        metrics: Some(json!({"accuracy": 0.95})), // Add metrics to avoid atom type rule
        provenance: json!({}),
        signature: vec![1, 2, 3],
        artifact_tree_hash: None,
            artifact_inline: None,
    };
    
    match pipeline.evaluate_atom(&short_atom) {
        mote::acceptance::AcceptanceDecision::Reject(reason) => {
            assert!(reason.contains("too short"));
        },
        _ => panic!("Expected rejection for short statement"),
    }
    
    // Test statement too long (but provide metrics to avoid atom type rule triggering first)
    let long_atom = AtomInput {
        atom_type: AtomType::Finding,
        domain: "test".to_string(),
        statement: "a".repeat(10001), // More than 10000 characters
        conditions: json!({}),
        metrics: Some(json!({"accuracy": 0.95})), // Add metrics to avoid atom type rule
        provenance: json!({}),
        signature: vec![1, 2, 3],
        artifact_tree_hash: None,
            artifact_inline: None,
    };
    
    match pipeline.evaluate_atom(&long_atom) {
        mote::acceptance::AcceptanceDecision::Reject(reason) => {
            assert!(reason.contains("too long"));
        },
        _ => panic!("Expected rejection for long statement"),
    }
    
    // Test valid length (but provide metrics to avoid atom type rule triggering first)
    let valid_length_atom = AtomInput {
        atom_type: AtomType::Finding,
        domain: "test".to_string(),
        statement: "This is a very long statement that exceeds the maximum allowed length for research findings and should therefore be rejected by the acceptance pipeline validation rules".to_string(),
        conditions: json!({}),
        metrics: Some(json!({"accuracy": 0.95})), // Add metrics to avoid atom type rule
        provenance: json!({}),
        signature: vec![1, 2, 3],
        artifact_tree_hash: None,
            artifact_inline: None,
    };
    
    match pipeline.evaluate_atom(&valid_length_atom) {
        mote::acceptance::AcceptanceDecision::Accept => {},
        _ => panic!("Expected acceptance for valid statement"),
    }
}

#[tokio::test]
async fn test_acceptance_pipeline_required_fields_validation() {
    let pipeline = mote::acceptance::AcceptancePipeline::new();
    
    // Test missing domain
    let no_domain_atom = AtomInput {
        atom_type: AtomType::Finding,
        domain: "".to_string(),
        statement: "Valid statement length".to_string(),
        conditions: json!({}),
        metrics: None,
        provenance: json!({}),
        signature: vec![1, 2, 3],
        artifact_tree_hash: None,
            artifact_inline: None,
    };
    
    match pipeline.evaluate_atom(&no_domain_atom) {
        mote::acceptance::AcceptanceDecision::Reject(reason) => {
            assert!(reason.contains("Domain is required"));
        },
        _ => panic!("Expected rejection for missing domain"),
    }
    
    // Test missing statement
    let no_statement_atom = AtomInput {
        atom_type: AtomType::Finding,
        domain: "test".to_string(),
        statement: "".to_string(),
        conditions: json!({}),
        metrics: None,
        provenance: json!({}),
        signature: vec![],
        artifact_tree_hash: None,
            artifact_inline: None,
    };
    
    match pipeline.evaluate_atom(&no_statement_atom) {
        mote::acceptance::AcceptanceDecision::Reject(reason) => {
            println!("Actual rejection reason: '{}'", reason);
            assert!(reason.contains("Statement too short") || reason.contains("Statement is required"));
        },
        _ => panic!("Expected rejection for missing statement"),
    }
    
    // Test missing signature
    let no_signature_atom = AtomInput {
        atom_type: AtomType::Finding,
        domain: "test".to_string(),
        statement: "Valid statement length".to_string(),
        conditions: json!({}),
        metrics: None,
        provenance: json!({}),
        signature: vec![],
        artifact_tree_hash: None,
            artifact_inline: None,
    };
    
    match pipeline.evaluate_atom(&no_signature_atom) {
        mote::acceptance::AcceptanceDecision::Reject(reason) => {
            assert!(reason.contains("Signature is required"));
        },
        _ => panic!("Expected rejection for missing signature"),
    }
}

#[tokio::test]
async fn test_acceptance_pipeline_domain_validation() {
    let pipeline = mote::acceptance::AcceptancePipeline::new();
    
    // Test invalid domain characters
    let invalid_domain_atom = AtomInput {
        atom_type: AtomType::Finding,
        domain: "test@domain".to_string(), // Invalid character @
        statement: "Valid statement length".to_string(),
        conditions: json!({}),
        metrics: None,
        provenance: json!({}),
        signature: vec![1, 2, 3],
        artifact_tree_hash: None,
            artifact_inline: None,
    };
    
    match pipeline.evaluate_atom(&invalid_domain_atom) {
        mote::acceptance::AcceptanceDecision::Reject(reason) => {
            assert!(reason.contains("invalid characters"));
        },
        _ => panic!("Expected rejection for invalid domain characters"),
    }
    
    // Test domain too long
    let long_domain_atom = AtomInput {
        atom_type: AtomType::Finding,
        domain: "a".repeat(101), // More than 100 characters
        statement: "Valid statement length".to_string(),
        conditions: json!({}),
        metrics: None,
        provenance: json!({}),
        signature: vec![1, 2, 3],
        artifact_tree_hash: None,
            artifact_inline: None,
    };
    
    match pipeline.evaluate_atom(&long_domain_atom) {
        mote::acceptance::AcceptanceDecision::Reject(reason) => {
            assert!(reason.contains("too long"));
        },
        _ => panic!("Expected rejection for long domain"),
    }
    
    // Test valid domain (but provide metrics to avoid atom type rule triggering first)
    let valid_domain_atom = AtomInput {
        atom_type: AtomType::Finding,
        domain: "test-domain_123".to_string(),
        statement: "Valid statement length".to_string(),
        conditions: json!({}),
        metrics: Some(json!({"accuracy": 0.95})), // Add metrics to avoid atom type rule
        provenance: json!({}),
        signature: vec![1, 2, 3],
        artifact_tree_hash: None,
            artifact_inline: None,
    };
    
    match pipeline.evaluate_atom(&valid_domain_atom) {
        mote::acceptance::AcceptanceDecision::Accept => {},
        _ => panic!("Expected acceptance for valid domain"),
    }
}

#[tokio::test]
async fn test_acceptance_pipeline_atom_type_limits() {
    let pipeline = mote::acceptance::AcceptancePipeline::new();
    
    // Test hypothesis without conditions (should be queued)
    let hypothesis_no_conditions = AtomInput {
        atom_type: AtomType::Hypothesis,
        domain: "test".to_string(),
        statement: "Test hypothesis without conditions".to_string(),
        conditions: json!({}), // Empty conditions
        metrics: None,
        provenance: json!({}),
        signature: vec![1, 2, 3],
        artifact_tree_hash: None,
            artifact_inline: None,
    };
    
    match pipeline.evaluate_atom(&hypothesis_no_conditions) {
        mote::acceptance::AcceptanceDecision::Queue(reason) => {
            assert!(reason.contains("without conditions"));
        },
        _ => panic!("Expected queue for hypothesis without conditions"),
    }
    
    // Test finding without metrics (should be queued)
    let finding_no_metrics = AtomInput {
        atom_type: AtomType::Finding,
        domain: "test".to_string(),
        statement: "Test finding without metrics".to_string(),
        conditions: json!({"temperature": 25.0}),
        metrics: None, // No metrics
        provenance: json!({}),
        signature: vec![1, 2, 3],
        artifact_tree_hash: None,
            artifact_inline: None,
    };
    
    match pipeline.evaluate_atom(&finding_no_metrics) {
        mote::acceptance::AcceptanceDecision::Queue(reason) => {
            assert!(reason.contains("without metrics"));
        },
        _ => panic!("Expected queue for finding without metrics"),
    }
    
    // Test valid hypothesis with conditions
    let valid_hypothesis = AtomInput {
        atom_type: AtomType::Hypothesis,
        domain: "test".to_string(),
        statement: "Test hypothesis with conditions".to_string(),
        conditions: json!({"temperature": 25.0, "pressure": 101.3}),
        metrics: None,
        provenance: json!({}),
        signature: vec![1, 2, 3],
        artifact_tree_hash: None,
            artifact_inline: None,
    };
    
    match pipeline.evaluate_atom(&valid_hypothesis) {
        mote::acceptance::AcceptanceDecision::Accept => {},
        _ => panic!("Expected acceptance for valid hypothesis"),
    }
    
    // Test valid finding with metrics
    let valid_finding = AtomInput {
        atom_type: AtomType::Finding,
        domain: "test".to_string(),
        statement: "Test finding with metrics".to_string(),
        conditions: json!({"temperature": 25.0}),
        metrics: Some(json!({"accuracy": 0.95})),
        provenance: json!({}),
        signature: vec![1, 2, 3],
        artifact_tree_hash: None,
            artifact_inline: None,
    };
    
    match pipeline.evaluate_atom(&valid_finding) {
        mote::acceptance::AcceptanceDecision::Accept => {},
        _ => panic!("Expected acceptance for valid finding"),
    }
}

#[tokio::test]
async fn test_acceptance_pipeline_complete_flow() {
    let pipeline = mote::acceptance::AcceptancePipeline::new();
    
    // Test a completely valid atom that should pass all checks
    let valid_atom = AtomInput {
        atom_type: AtomType::Finding,
        domain: "research-domain".to_string(),
        statement: "We observed a significant correlation between temperature and reaction rate".to_string(),
        conditions: json!({
            "temperature": 25.0,
            "pressure": 101.3,
            "catalyst": "platinum"
        }),
        metrics: Some(json!({
            "correlation_coefficient": 0.87,
            "p_value": 0.002,
            "sample_size": 150
        })),
        provenance: json!({
            "experiment_id": "exp_123",
            "researcher": "Dr. Smith",
            "lab": "Chemistry Lab A"
        }),
        signature: vec![1, 2, 3, 4, 5],
        artifact_tree_hash: None,
            artifact_inline: None,
    };
    
    match pipeline.evaluate_atom(&valid_atom) {
        mote::acceptance::AcceptanceDecision::Accept => {},
        _ => panic!("Expected acceptance for completely valid atom"),
    }
}
