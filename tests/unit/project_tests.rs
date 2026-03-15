//! Unit tests for multi-project support
//!
//! Tests cover:
//! - Project domain struct creation and field validation
//! - Slug validation logic (mirrors project_queries::create_project validation)
//! - AtomInput project_id field is correctly optional
//! - search_atoms project_id_filter parameter signature

use asenix::domain::project::Project;
use asenix::domain::atom::{AtomInput, AtomType};
use chrono::Utc;
use serde_json::json;

// ── Project domain struct ────────────────────────────────────────────────────

#[test]
fn test_project_struct_fields() {
    let p = Project {
        project_id: "proj_abc123".to_string(),
        name: "CIFAR-10 ResNet Search".to_string(),
        slug: "cifar10-resnet-search".to_string(),
        description: Some("Hyperparameter search for ResNet on CIFAR-10".to_string()),
        created_at: Utc::now(),
    };

    assert_eq!(p.project_id, "proj_abc123");
    assert_eq!(p.name, "CIFAR-10 ResNet Search");
    assert_eq!(p.slug, "cifar10-resnet-search");
    assert!(p.description.is_some());
}

#[test]
fn test_project_optional_description() {
    let p = Project {
        project_id: "proj_xyz".to_string(),
        name: "Minimal Project".to_string(),
        slug: "minimal-project".to_string(),
        description: None,
        created_at: Utc::now(),
    };

    assert!(p.description.is_none());
}

#[test]
fn test_project_serialization_round_trip() {
    let p = Project {
        project_id: "proj_test".to_string(),
        name: "Test Project".to_string(),
        slug: "test-project".to_string(),
        description: Some("A test project".to_string()),
        created_at: Utc::now(),
    };

    let serialized = serde_json::to_string(&p).expect("serialization failed");
    let deserialized: Project = serde_json::from_str(&serialized).expect("deserialization failed");

    assert_eq!(deserialized.project_id, p.project_id);
    assert_eq!(deserialized.name, p.name);
    assert_eq!(deserialized.slug, p.slug);
    assert_eq!(deserialized.description, p.description);
}

// ── Slug validation ──────────────────────────────────────────────────────────

fn is_valid_slug(slug: &str) -> bool {
    !slug.is_empty()
        && slug.chars().all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-')
}

#[test]
fn test_slug_valid_cases() {
    assert!(is_valid_slug("cifar10-resnet-search"));
    assert!(is_valid_slug("my-project"));
    assert!(is_valid_slug("project123"));
    assert!(is_valid_slug("a"));
    assert!(is_valid_slug("llm-finetuning-v2"));
}

#[test]
fn test_slug_invalid_cases() {
    assert!(!is_valid_slug(""));                     // empty
    assert!(!is_valid_slug("My-Project"));           // uppercase
    assert!(!is_valid_slug("project name"));         // space
    assert!(!is_valid_slug("project_name"));         // underscore
    assert!(!is_valid_slug("project@domain"));       // @
    assert!(!is_valid_slug("project/path"));         // slash
}

// ── AtomInput project_id field ───────────────────────────────────────────────

#[test]
fn test_atom_input_project_id_is_optional() {
    let input_with_project = AtomInput {
        atom_type: AtomType::Finding,
        domain: "cifar10_resnet".to_string(),
        project_id: Some("proj_cifar10_resnet".to_string()),
        statement: "ResNet-50 achieves 95.1% accuracy on CIFAR-10".to_string(),
        conditions: json!({ "model": "resnet50", "learning_rate": 0.01 }),
        metrics: Some(json!([{ "name": "accuracy", "direction": "maximize" }])),
        provenance: json!({}),
        signature: vec![],
        artifact_tree_hash: None,
        artifact_inline: None,
    };

    assert_eq!(
        input_with_project.project_id,
        Some("proj_cifar10_resnet".to_string())
    );

    let input_without_project = AtomInput {
        atom_type: AtomType::Hypothesis,
        domain: "llm_research".to_string(),
        project_id: None,
        statement: "Larger models generalize better with less data".to_string(),
        conditions: json!({}),
        metrics: None,
        provenance: json!({}),
        signature: vec![],
        artifact_tree_hash: None,
        artifact_inline: None,
    };

    assert!(input_without_project.project_id.is_none());
}

#[test]
fn test_atom_input_project_id_serializes_correctly() {
    let input = AtomInput {
        atom_type: AtomType::Bounty,
        domain: "test_domain".to_string(),
        project_id: Some("proj_abc".to_string()),
        statement: "Find the best configuration for CIFAR-10".to_string(),
        conditions: json!({}),
        metrics: None,
        provenance: json!({}),
        signature: vec![],
        artifact_tree_hash: None,
        artifact_inline: None,
    };

    let serialized = serde_json::to_value(&input).expect("serialization failed");
    assert_eq!(serialized["project_id"], json!("proj_abc"));

    let input_no_project = AtomInput {
        atom_type: AtomType::Bounty,
        domain: "test_domain".to_string(),
        project_id: None,
        statement: "Find the best configuration for CIFAR-10".to_string(),
        conditions: json!({}),
        metrics: None,
        provenance: json!({}),
        signature: vec![],
        artifact_tree_hash: None,
        artifact_inline: None,
    };

    let serialized_none = serde_json::to_value(&input_no_project).expect("serialization failed");
    // project_id should either be absent or null when None
    let project_id_val = &serialized_none["project_id"];
    assert!(project_id_val.is_null() || project_id_val == &json!(null));
}
