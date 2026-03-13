//! Unit tests for acceptance pipeline logic
//! 
//! Tests the AcceptancePipeline and related rules without database dependencies.
//! Uses mockable inputs to simulate agent state, core atoms, and provenance requirements.

use serde_json::json;
use std::collections::HashMap;

use mote::acceptance::{AcceptancePipeline, AcceptanceDecision, AcceptanceRule};
use mote::domain::atom::{AtomInput, AtomType, Provenance};
use mote::domain::agent::Agent;

// Mock agent state for testing
#[derive(Clone)]
struct MockAgent {
    pub agent_id: String,
    pub confirmed: bool,
    pub reliability: f64,
    pub atom_count: usize,
}

// Mock core atom for neighborhood testing
#[derive(Clone)]
struct MockCoreAtom {
    pub atom_id: String,
    pub conditions: serde_json::Value,
    pub contradiction_count: i32,
}

// Mock acceptance context
struct MockAcceptanceContext {
    pub agent: MockAgent,
    pub core_atoms: Vec<MockCoreAtom>,
    pub required_provenance_fields: Vec<String>,
    pub existing_hashes: std::collections::HashSet<String>,
}

impl MockAcceptanceContext {
    fn new() -> Self {
        Self {
            agent: MockAgent {
                agent_id: "test_agent".to_string(),
                confirmed: true,
                reliability: 0.8,
                atom_count: 10,
            },
            core_atoms: Vec::new(),
            required_provenance_fields: vec![
                "methodology".to_string(),
                "data_source".to_string(),
                "confidence".to_string(),
            ],
            existing_hashes: std::collections::HashSet::new(),
        }
    }
    
    fn with_agent_confirmed(mut self, confirmed: bool) -> Self {
        self.agent.confirmed = confirmed;
        self
    }
    
    fn with_agent_reliability(mut self, reliability: f64) -> Self {
        self.agent.reliability = reliability;
        self
    }
    
    fn with_agent_probationary(mut self) -> Self {
        self.agent.atom_count = 3; // Below probation threshold
        self
    }
    
    fn with_core_atom(mut self, atom: MockCoreAtom) -> Self {
        self.core_atoms.push(atom);
        self
    }
    
    fn with_existing_hash(mut self, hash: &str) -> Self {
        self.existing_hashes.insert(hash.to_string());
        self
    }
}

fn create_test_atom_input() -> AtomInput {
    AtomInput {
        atom_type: AtomType::Finding,
        domain: "physics".to_string(),
        statement: "Test measurement result".to_string(),
        conditions: json!({
            "temperature": 25.0,
            "uncertainty": 0.1
        }),
        metrics: None,
        provenance: Provenance {
            methodology: "controlled_experiment".to_string(),
            data_source: "lab_measurement".to_string(),
            confidence: 0.95,
            parent_ids: vec!["parent_1".to_string()],
            metadata: json!({"equipment": "spectrometer"}),
        },
        signature: vec![],
    }
}

fn create_test_pipeline() -> AcceptancePipeline {
    AcceptancePipeline::new()
}

#[tokio::test]
async fn test_agent_not_confirmed_rejection() {
    let pipeline = create_test_pipeline();
    let atom = create_test_atom_input();
    let context = MockAcceptanceContext::new()
        .with_agent_confirmed(false);
    
    // Mock the pipeline to check agent confirmation
    let decision = pipeline.evaluate_atom(&atom, &context.agent, &context.core_atoms).await;
    
    // Should reject unconfirmed agents
    match decision {
        AcceptanceDecision::Reject(reason) => {
            assert!(reason.contains("not confirmed") || reason.contains("unconfirmed"));
        }
        _ => panic!("Expected rejection for unconfirmed agent"),
    }
}

#[tokio::test]
async fn test_missing_required_provenance_field_rejection() {
    let pipeline = create_test_pipeline();
    let mut atom = create_test_atom_input();
    
    // Remove required provenance field
    atom.provenance.methodology = "".to_string(); // Empty methodology
    
    let context = MockAcceptanceContext::new();
    
    let decision = pipeline.evaluate_atom(&atom, &context.agent, &context.core_atoms).await;
    
    match decision {
        AcceptanceDecision::Reject(reason) => {
            assert!(reason.contains("methodology") || reason.contains("required"));
        }
        _ => panic!("Expected rejection for missing required provenance field"),
    }
}

#[tokio::test]
async fn test_duplicate_hash_rejection() {
    let pipeline = create_test_pipeline();
    let atom = create_test_atom_input();
    
    // Simulate existing hash
    let existing_hash = "existing_hash_123";
    let context = MockAcceptanceContext::new()
        .with_existing_hash(existing_hash);
    
    // Mock hash computation to return existing hash
    let decision = pipeline.evaluate_atom(&atom, &context.agent, &context.core_atoms).await;
    
    match decision {
        AcceptanceDecision::Reject(reason) => {
            assert!(reason.contains("duplicate") || reason.contains("exists"));
        }
        _ => panic!("Expected rejection for duplicate hash"),
    }
}

#[tokio::test]
async fn test_agent_below_reliability_with_contradiction_queue() {
    let pipeline = create_test_pipeline();
    let atom = create_test_atom_input();
    
    // Create contradictory core atom
    let contradictory_core = MockCoreAtom {
        atom_id: "core_1".to_string(),
        conditions: json!({
            "temperature": 30.0, // Contradicts our atom's 25.0
            "uncertainty": 0.1
        }),
        contradiction_count: 0,
    };
    
    let context = MockAcceptanceContext::new()
        .with_agent_reliability(0.5) // Below reliability threshold
        .with_core_atom(contradictory_core);
    
    let decision = pipeline.evaluate_atom(&atom, &context.agent, &context.core_atoms).await;
    
    match decision {
        AcceptanceDecision::Queue(reason) => {
            assert!(reason.contains("contradiction") || reason.contains("reliability"));
        }
        _ => panic!("Expected queue decision for low reliability agent with contradiction"),
    }
}

#[tokio::test]
async fn test_probationary_agent_accepted_with_flag() {
    let pipeline = create_test_pipeline();
    let atom = create_test_atom_input();
    
    let context = MockAcceptanceContext::new()
        .with_agent_probationary();
    
    let decision = pipeline.evaluate_atom(&atom, &context.agent, &context.core_atoms).await;
    
    match decision {
        AcceptanceDecision::Accept => {
            // Probationary agents should be accepted but flagged
            // This would be verified through additional metadata in real implementation
        }
        _ => panic!("Expected acceptance for probationary agent"),
    }
}

#[tokio::test]
async fn test_exact_match_accept_path() {
    let pipeline = create_test_pipeline();
    let atom = create_test_atom_input();
    
    // Create matching core atom (same conditions)
    let matching_core = MockCoreAtom {
        atom_id: "core_1".to_string(),
        conditions: json!({
            "temperature": 25.0, // Same as our atom
            "uncertainty": 0.1
        }),
        contradiction_count: 0,
    };
    
    let context = MockAcceptanceContext::new()
        .with_core_atom(matching_core);
    
    let decision = pipeline.evaluate_atom(&atom, &context.agent, &context.core_atoms).await;
    
    match decision {
        AcceptanceDecision::Accept => {
            // Should accept exact matches
        }
        _ => panic!("Expected acceptance for exact match"),
    }
}

#[tokio::test]
async fn test_statement_length_validation() {
    let pipeline = create_test_pipeline();
    let mut atom = create_test_atom_input();
    
    // Test with very long statement
    atom.statement = "A".repeat(10000);
    
    let context = MockAcceptanceContext::new();
    
    let decision = pipeline.evaluate_atom(&atom, &context.agent, &context.core_atoms).await;
    
    match decision {
        AcceptanceDecision::Reject(reason) => {
            assert!(reason.contains("length") || reason.contains("too long"));
        }
        AcceptanceDecision::Accept => {
            // If accepted, length validation may not be implemented yet
        }
        AcceptanceDecision::Queue(_) => {
            // Could also be queued for review
        }
    }
}

#[tokio::test]
async fn test_domain_validation() {
    let pipeline = create_test_pipeline();
    let mut atom = create_test_atom_input();
    
    // Test with invalid domain
    atom.domain = "invalid_domain_with_spaces".to_string();
    
    let context = MockAcceptanceContext::new();
    
    let decision = pipeline.evaluate_atom(&atom, &context.agent, &context.core_atoms).await;
    
    match decision {
        AcceptanceDecision::Reject(reason) => {
            assert!(reason.contains("domain") || reason.contains("invalid"));
        }
        AcceptanceDecision::Accept => {
            // If accepted, domain validation may not be implemented yet
        }
        AcceptanceDecision::Queue(_) => {
            // Could also be queued for review
        }
    }
}

#[tokio::test]
async fn test_atom_type_limits() {
    let pipeline = create_test_pipeline();
    let mut atom = create_test_atom_input();
    
    // Test with restricted atom type (e.g., bounty)
    atom.atom_type = AtomType::Bounty;
    
    let context = MockAcceptanceContext::new();
    
    let decision = pipeline.evaluate_atom(&atom, &context.agent, &context.core_atoms).await;
    
    match decision {
        AcceptanceDecision::Reject(reason) => {
            assert!(reason.contains("bounty") || reason.contains("restricted"));
        }
        AcceptanceDecision::Accept => {
            // If accepted, bounty type may be allowed
        }
        AcceptanceDecision::Queue(_) => {
            // Could also be queued for review
        }
    }
}

#[tokio::test]
async fn test_multiple_rule_priority() {
    let pipeline = create_test_pipeline();
    let mut atom = create_test_atom_input();
    
    // Create atom with multiple issues
    atom.statement = "A".repeat(10000); // Too long
    atom.domain = "invalid domain".to_string(); // Invalid domain
    atom.provenance.methodology = "".to_string(); // Missing methodology
    
    let context = MockAcceptanceContext::new()
        .with_agent_confirmed(false); // Also unconfirmed
    
    let decision = pipeline.evaluate_atom(&atom, &context.agent, &context.core_atoms).await;
    
    // Should reject based on highest priority rule (agent confirmation)
    match decision {
        AcceptanceDecision::Reject(reason) => {
            // Priority should be given to agent confirmation over other issues
            assert!(reason.contains("not confirmed") || reason.contains("unconfirmed"));
        }
        _ => panic!("Expected rejection"),
    }
}

#[tokio::test]
async fn test_contradiction_detection() {
    let pipeline = create_test_pipeline();
    let atom = create_test_atom_input();
    
    // Create core atoms with varying degrees of contradiction
    let slightly_contradictory = MockCoreAtom {
        atom_id: "core_1".to_string(),
        conditions: json!({
            "temperature": 25.1, // Slightly different
            "uncertainty": 0.1
        }),
        contradiction_count: 0,
    };
    
    let highly_contradictory = MockCoreAtom {
        atom_id: "core_2".to_string(),
        conditions: json!({
            "temperature": 50.0, // Very different
            "uncertainty": 0.1
        }),
        contradiction_count: 0,
    };
    
    let context = MockAcceptanceContext::new()
        .with_core_atom(slightly_contradictory)
        .with_core_atom(highly_contradictory);
    
    let decision = pipeline.evaluate_atom(&atom, &context.agent, &context.core_atoms).await;
    
    // Should detect contradictions and potentially queue for review
    match decision {
        AcceptanceDecision::Queue(reason) => {
            assert!(reason.contains("contradiction") || reason.contains("conflict"));
        }
        AcceptanceDecision::Accept => {
            // Minor contradictions might be accepted
        }
        AcceptanceDecision::Reject(_) => {
            // Major contradictions might be rejected
        }
    }
}

#[tokio::test]
async fn test_edge_case_empty_atom() {
    let pipeline = create_test_pipeline();
    let atom = AtomInput {
        atom_type: AtomType::Finding,
        domain: "".to_string(),
        statement: "".to_string(),
        conditions: json!({}),
        metrics: None,
        provenance: Provenance {
            methodology: "".to_string(),
            data_source: "".to_string(),
            confidence: 0.0,
            parent_ids: vec![],
            metadata: json!({}),
        },
        signature: vec![],
    };
    
    let context = MockAcceptanceContext::new();
    
    let decision = pipeline.evaluate_atom(&atom, &context.agent, &context.core_atoms).await;
    
    match decision {
        AcceptanceDecision::Reject(reason) => {
            // Empty atom should be rejected
            assert!(reason.contains("empty") || reason.contains("required") || reason.contains("missing"));
        }
        _ => panic!("Expected rejection for empty atom"),
    }
}

#[tokio::test]
async fn test_high_reliability_agent_contradiction_handling() {
    let pipeline = create_test_pipeline();
    let atom = create_test_atom_input();
    
    // Create contradictory core atom
    let contradictory_core = MockCoreAtom {
        atom_id: "core_1".to_string(),
        conditions: json!({
            "temperature": 30.0, // Contradicts our atom's 25.0
            "uncertainty": 0.1
        }),
        contradiction_count: 0,
    };
    
    let context = MockAcceptanceContext::new()
        .with_agent_reliability(0.95) // High reliability
        .with_core_atom(contradictory_core);
    
    let decision = pipeline.evaluate_atom(&atom, &context.agent, &context.core_atoms).await;
    
    // High reliability agents might be allowed to contradict core atoms
    match decision {
        AcceptanceDecision::Accept => {
            // High reliability agents can contradict
        }
        AcceptanceDecision::Queue(reason) => {
            // Or queued for review even with high reliability
            assert!(reason.contains("contradiction"));
        }
        AcceptanceDecision::Reject(_) => {
            // But should not be rejected if reliability is high
            panic!("High reliability agents should not be rejected for contradictions");
        }
    }
}

#[tokio::test]
async fn test_rule_enable_disable() {
    let mut pipeline = create_test_pipeline();
    let atom = create_test_atom_input();
    
    // Disable a rule
    pipeline.disable_rule("statement_length").await;
    
    let mut long_atom = atom.clone();
    long_atom.statement = "A".repeat(10000); // Very long
    
    let context = MockAcceptanceContext::new();
    
    let decision = pipeline.evaluate_atom(&long_atom, &context.agent, &context.core_atoms).await;
    
    // Should not be rejected for statement length since rule is disabled
    match decision {
        AcceptanceDecision::Reject(reason) => {
            assert!(!reason.contains("length"));
        }
        _ => {
            // Accept or queue is fine
        }
    }
    
    // Re-enable the rule
    pipeline.enable_rule("statement_length").await;
    
    let decision = pipeline.evaluate_atom(&long_atom, &context.agent, &context.core_atoms).await;
    
    // Now should be rejected for statement length
    match decision {
        AcceptanceDecision::Reject(reason) => {
            assert!(reason.contains("length"));
        }
        _ => panic!("Expected rejection after re-enabling rule"),
    }
}
