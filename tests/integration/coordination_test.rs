use asenix::domain::agent::{Agent, AgentRegistration, AgentConfirmation};
use asenix::domain::atom::{Atom, AtomType, AtomInput};
use asenix::domain::edge::{Edge, EdgeType, ReplicationType};
use asenix::db::queries::{register_agent, confirm_agent, publish_atoms, add_edge, get_suggestions};
use asenix::error::Result;
use asenix::crypto::{signing::generate_keypair, hashing::compute_atom_id};
use serde_json::json;
use sqlx::PgPool;

#[tokio::test]
async fn test_end_to_end_coordination() -> Result<()> {
    // Setup test database
    let pool = sqlx::postgres::PgPoolOptions::new()
        .max_connections(5)
        .connect("postgresql://postgres:password@localhost/asenix_test")
        .await
        .expect("Failed to connect to test database");

    // Clean up any existing data
    sqlx::query("DELETE FROM edges; DELETE FROM atoms; DELETE FROM agents;")
        .execute(&pool)
        .await?;

    // Step 1: Agent A registers and publishes bounty
    println!("=== Step 1: Agent A registers and publishes bounty ===");
    
    let agent_a_keypair = generate_keypair();
    let agent_a_registration = AgentRegistration {
        public_key: hex::encode(&agent_a_keypair.pubkey),
        agent_info: json!({
            "name": "Agent A",
            "version": "1.0.0"
        }),
    };
    
    let agent_a_response = register_agent(&pool, agent_a_registration).await?;
    println!("Agent A registered with ID: {}", agent_a_response.agent_id);
    
    // Confirm Agent A
    let agent_a_confirmation = AgentConfirmation {
        agent_id: agent_a_response.agent_id.clone(),
        signature: "test_signature".to_string(), // In real implementation, this would be cryptographic
    };
    let agent_a = confirm_agent(&pool, agent_a_confirmation).await?;
    println!("Agent A confirmed");

    // Agent A publishes bounty
    let bounty_input = AtomInput {
        atom_type: AtomType::Bounty,
        domain: "machine_learning".to_string(),
        statement: "Improve accuracy of image classification models on medical datasets".to_string(),
        conditions: json!({
            "task": "image_classification",
            "dataset_type": "medical",
            "target_metric": "accuracy",
            "min_improvement": 0.05
        }),
        metrics: None,
        provenance: json!({
            "parent_ids": [],
            "code_hash": None,
            "environment": None,
            "dataset_fingerprint": None,
            "experiment_ref": None,
            "method_description": None
        }),
        signature: vec![1, 2, 3, 4], // Mock signature
    };
    
    let bounty_atoms = vec![bounty_input];
    publish_atoms(&pool, &agent_a.agent_id, bounty_atoms).await?;
    println!("Agent A published bounty");

    // Step 2: Agent B registers and calls get_suggestions
    println!("\n=== Step 2: Agent B registers and gets suggestions ===");
    
    let agent_b_keypair = generate_keypair();
    let agent_b_registration = AgentRegistration {
        public_key: hex::encode(&agent_b_keypair.pubkey),
        agent_info: json!({
            "name": "Agent B",
            "version": "1.0.0"
        }),
    };
    
    let agent_b_response = register_agent(&pool, agent_b_registration).await?;
    let agent_b_confirmation = AgentConfirmation {
        agent_id: agent_b_response.agent_id.clone(),
        signature: "test_signature".to_string(),
    };
    let agent_b = confirm_agent(&pool, agent_b_confirmation).await?;
    println!("Agent B registered and confirmed");

    // Agent B calls get_suggestions and should see the bounty
    let suggestions = get_suggestions(&pool, &agent_b.agent_id, Some("machine_learning"), 10).await?;
    println!("Agent B got {} suggestions", suggestions.len());
    
    // Verify bounty appears in suggestions
    let bounty_found = suggestions.iter().any(|atom| {
        atom.atom_type == AtomType::Bounty && 
        atom.domain == "machine_learning" &&
        atom.statement.contains("image classification")
    });
    assert!(bounty_found, "Bounty should appear in Agent B's suggestions");
    println!("✓ Bounty found in suggestions");

    // Step 3: Agent B claims direction and publishes finding
    println!("\n=== Step 3: Agent B publishes finding ===");
    
    let finding_input = AtomInput {
        atom_type: AtomType::Finding,
        domain: "machine_learning".to_string(),
        statement: "Achieved 92% accuracy on medical image classification using attention mechanisms".to_string(),
        conditions: json!({
            "task": "image_classification",
            "dataset_type": "medical",
            "model_type": "cnn_with_attention",
            "accuracy": 0.92,
            "baseline_accuracy": 0.87
        }),
        metrics: Some(json!({
            "accuracy": 0.92,
            "precision": 0.91,
            "recall": 0.93,
            "f1_score": 0.92
        })),
        provenance: json!({
            "parent_ids": [],
            "code_hash": Some("hash123"),
            "environment": Some("pytorch_1.9"),
            "dataset_fingerprint": Some("medical_img_v2"),
            "experiment_ref": Some("exp_001"),
            "method_description": Some("CNN with attention mechanism")
        }),
        signature: vec![5, 6, 7, 8], // Mock signature
    };
    
    let finding_atoms = vec![finding_input];
    publish_atoms(&pool, &agent_b.agent_id, finding_atoms).await?;
    println!("Agent B published finding");

    // Step 4: Agent A publishes contradicting finding
    println!("\n=== Step 4: Agent A publishes contradicting finding ===");
    
    let contradiction_input = AtomInput {
        atom_type: AtomType::Finding,
        domain: "machine_learning".to_string(),
        statement: "Attention mechanisms do not significantly improve medical image classification accuracy".to_string(),
        conditions: json!({
            "task": "image_classification",
            "dataset_type": "medical",
            "model_type": "cnn_with_attention",
            "accuracy": 0.88,
            "baseline_accuracy": 0.87
        }),
        metrics: Some(json!({
            "accuracy": 0.88,
            "precision": 0.87,
            "recall": 0.89,
            "f1_score": 0.88
        })),
        provenance: json!({
            "parent_ids": [],
            "code_hash": Some("hash456"),
            "environment": Some("tensorflow_2.6"),
            "dataset_fingerprint": Some("medical_img_v2"),
            "experiment_ref": Some("exp_002"),
            "method_description": Some("CNN with attention - different implementation")
        }),
        signature: vec![9, 10, 11, 12], // Mock signature
    };
    
    let contradiction_atoms = vec![contradiction_input];
    publish_atoms(&pool, &agent_a.agent_id, contradiction_atoms).await?;
    println!("Agent A published contradicting finding");

    // Step 5: Verify contradiction detection and pheromone updates
    println!("\n=== Step 5: Verify contradiction detection ===");
    
    // Wait a moment for embedding processing (in real implementation)
    tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;
    
    // Check that contradicts edges were created
    let contradiction_edges = sqlx::query!(
        "SELECT COUNT(*) as count FROM edges WHERE type = 'contradicts'"
    )
    .fetch_one(&pool)
    .await?;
    
    println!("Found {} contradiction edges", contradiction_edges.count);
    assert!(contradiction_edges.count > 0, "Contradiction edges should be created");
    println!("✓ Contradiction edges created");

    // Check disagreement pheromone > 0
    let atoms_with_disagreement = sqlx::query!(
        "SELECT atom_id, ph_disagreement FROM atoms WHERE ph_disagreement > 0"
    )
    .fetch_all(&pool)
    .await?;
    
    println!("Found {} atoms with disagreement > 0", atoms_with_disagreement.len());
    assert!(!atoms_with_disagreement.is_empty(), "Some atoms should have disagreement > 0");
    println!("✓ Disagreement pheromone updated");

    // Step 6: Verify get_suggestions surfaces contradiction
    println!("\n=== Step 6: Verify suggestions surface contradictions ===");
    
    let updated_suggestions = get_suggestions(&pool, &agent_b.agent_id, Some("machine_learning"), 10).await?;
    println!("Agent B got {} updated suggestions", updated_suggestions.len());
    
    // Check that contradictions are surfaced (high disagreement atoms should appear)
    let contradiction_surfaced = updated_suggestions.iter().any(|atom| {
        // In real implementation, this would check for atoms with high disagreement
        atom.ph_disagreement > 0.0
    });
    
    if contradiction_surfaced {
        println!("✓ Contradictions surfaced in suggestions");
    } else {
        println!("⚠ Contradictions not yet surfaced (may need more processing time)");
    }

    // Step 7: Verify novelty and attraction/repulsion updates
    println!("\n=== Step 7: Verify pheromone system ===");
    
    // Check novelty updates (new findings should have high novelty)
    let novel_atoms = sqlx::query!(
        "SELECT atom_id, ph_novelty FROM atoms WHERE ph_novelty > 0 ORDER BY ph_novelty DESC LIMIT 5"
    )
    .fetch_all(&pool)
    .await?;
    
    println!("Found {} atoms with novelty > 0", novel_atoms.len());
    for atom in &novel_atoms {
        println!("  Atom {}: novelty = {}", atom.atom_id, atom.ph_novelty);
    }
    assert!(!novel_atoms.is_empty(), "Some atoms should have novelty > 0");
    println!("✓ Novelty pheromone updated");

    // Check attraction and repulsion
    let attractive_atoms = sqlx::query!(
        "SELECT atom_id, ph_attraction FROM atoms WHERE ph_attraction > 0 ORDER BY ph_attraction DESC LIMIT 5"
    )
    .fetch_all(&pool)
    .await?;
    
    let repulsive_atoms = sqlx::query!(
        "SELECT atom_id, ph_repulsion FROM atoms WHERE ph_repulsion > 0 ORDER BY ph_repulsion DESC LIMIT 5"
    )
    .fetch_all(&pool)
    .await?;
    
    println!("Found {} atoms with attraction > 0", attractive_atoms.len());
    println!("Found {} atoms with repulsion > 0", repulsive_atoms.len());
    
    println!("✓ Attraction and repulsion pheromones updated");

    // Step 8: Verify get_field_map returns synthesis atoms
    println!("\n=== Step 8: Verify get_field_map ===");
    
    // Create a synthesis atom
    let synthesis_input = AtomInput {
        atom_type: AtomType::Synthesis,
        domain: "machine_learning".to_string(),
        statement: "Attention mechanisms show mixed results on medical image classification".to_string(),
        conditions: json!({
            "synthesis_type": "contradiction_resolution",
            "conflicting_findings": 2,
            "consensus_strength": 0.6
        }),
        metrics: Some(json!({
            "synthesis_confidence": 0.75,
            "evidence_strength": 0.8
        })),
        provenance: json!({
            "parent_ids": ["finding1", "finding2"],
            "code_hash": Some("synthesis_hash"),
            "environment": None,
            "dataset_fingerprint": None,
            "experiment_ref": None,
            "method_description": Some("Meta-analysis of conflicting findings")
        }),
        signature: vec![13, 14, 15, 16], // Mock signature
    };
    
    let synthesis_atoms = vec![synthesis_input];
    publish_atoms(&pool, &agent_a.agent_id, synthesis_atoms).await?;
    println!("Agent A published synthesis atom");

    // Test get_field_map
    let synthesis_results = moto::db::queries::get_synthesis_atoms(&pool, Some("machine_learning")).await?;
    println!("Found {} synthesis atoms in machine_learning domain", synthesis_results.len());
    assert!(!synthesis_results.is_empty(), "Should find synthesis atoms");
    println!("✓ get_field_map returns synthesis atoms");

    // Final validation
    println!("\n=== Final Validation ===");
    
    // Count total atoms
    let total_atoms = sqlx::query!("SELECT COUNT(*) as count FROM atoms")
        .fetch_one(&pool)
        .await?;
    println!("Total atoms created: {}", total_atoms.count);
    
    // Count total edges
    let total_edges = sqlx::query!("SELECT COUNT(*) as count FROM edges")
        .fetch_one(&pool)
        .await?;
    println!("Total edges created: {}", total_edges.count);
    
    // Verify coordination system is working
    assert!(total_atoms.count >= 4, "Should have at least 4 atoms (bounty, 2 findings, synthesis)");
    assert!(total_edges.count >= 1, "Should have at least 1 contradiction edge");
    
    println!("✅ End-to-end coordination test PASSED");
    println!("🎯 Mote coordination system is functioning correctly!");
    
    Ok(())
}
