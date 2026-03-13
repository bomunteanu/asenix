use super::{setup_test_app, initialize_session, make_tool_call};
use ed25519_dalek::Signer;
use serde_json::json;

/// Compute top-level signature for publish_atoms.
/// The server's authenticate_and_rate_limit removes the top-level "signature"
/// field from params and serializes the rest. call_tool adds "edges": null,
/// so the canonical form is: {"agent_id": ..., "atoms": [...], "edges": null}
fn sign_publish_atoms(
    signing_key: &ed25519_dalek::SigningKey,
    agent_id: &str,
    atoms: &serde_json::Value,
) -> String {
    let canonical = json!({
        "agent_id": agent_id,
        "atoms": atoms,
        "edges": null
    });
    let canonical_string = serde_json::to_string(&canonical).unwrap();
    let signature = signing_key.sign(canonical_string.as_bytes());
    hex::encode(signature.to_bytes())
}

#[tokio::test]
async fn test_end_to_end_coordination() -> Result<(), Box<dyn std::error::Error>> {
    let app = setup_test_app().await;
    let session_id = initialize_session(&app).await;

    // Step 1: Agent A registers and publishes bounty
    println!("=== Step 1: Agent A registers and publishes bounty ===");
    
    let agent_a_signing_key = ed25519_dalek::SigningKey::from_bytes(&[1u8; 32]);
    let agent_a_public_key = hex::encode(agent_a_signing_key.verifying_key().as_bytes());
    
    let registration_response = make_tool_call(&app, &session_id, "register_agent", json!({
        "public_key": agent_a_public_key
    }), json!(1)).await?;
    
    assert!(registration_response["result"].is_object());
    let agent_a_id = registration_response["result"]["agent_id"].as_str().unwrap();
    let agent_a_challenge = registration_response["result"]["challenge"].as_str().unwrap();
    println!("Agent A registered with ID: {}", agent_a_id);
    
    let agent_a_challenge_bytes = hex::decode(agent_a_challenge)?;
    let agent_a_signature = agent_a_signing_key.sign(&agent_a_challenge_bytes);
    let agent_a_signature_hex = hex::encode(agent_a_signature.to_bytes());
    
    let confirmation_response = make_tool_call(&app, &session_id, "confirm_agent", json!({
        "agent_id": agent_a_id,
        "signature": agent_a_signature_hex
    }), json!(2)).await?;
    
    assert!(confirmation_response["result"]["status"].as_str().unwrap() == "confirmed");
    println!("Agent A confirmed");

    // Agent A publishes bounty
    // Atom-level signature stored as Vec<u8> (not verified by server, just stored)
    let atom_sig: Vec<u8> = vec![0xAB; 64];
    
    let bounty_atoms = json!([{
        "atom_type": "bounty",
        "domain": "machine_learning",
        "statement": "Improve accuracy of image classification models on medical datasets",
        "conditions": {
            "task": "image_classification",
            "dataset_type": "medical",
            "target_metric": "accuracy",
            "min_improvement": 0.05
        },
        "metrics": null,
        "provenance": {
            "parent_ids": [],
            "code_hash": null,
            "environment": null,
            "dataset_fingerprint": null,
            "experiment_ref": null,
            "method_description": null
        },
        "signature": atom_sig
    }]);
    
    let bounty_top_sig = sign_publish_atoms(&agent_a_signing_key, agent_a_id, &bounty_atoms);
    
    let bounty_response = make_tool_call(&app, &session_id, "publish_atoms", json!({
        "agent_id": agent_a_id,
        "signature": bounty_top_sig,
        "atoms": bounty_atoms
    }), json!(3)).await?;
    
    println!("Bounty response: {}", serde_json::to_string_pretty(&bounty_response).unwrap());
    assert!(bounty_response["result"]["published_atoms"].as_array().unwrap().len() == 1);
    println!("Agent A published bounty");

    // Step 2: Agent B registers and calls get_suggestions
    println!("\n=== Step 2: Agent B registers and gets suggestions ===");
    
    let agent_b_signing_key = ed25519_dalek::SigningKey::from_bytes(&[2u8; 32]);
    let agent_b_public_key = hex::encode(agent_b_signing_key.verifying_key().as_bytes());
    
    let agent_b_registration = make_tool_call(&app, &session_id, "register_agent", json!({
        "public_key": agent_b_public_key
    }), json!(4)).await?;
    
    let agent_b_id = agent_b_registration["result"]["agent_id"].as_str().unwrap();
    let agent_b_challenge = agent_b_registration["result"]["challenge"].as_str().unwrap();
    
    let agent_b_challenge_bytes = hex::decode(agent_b_challenge)?;
    let agent_b_signature = agent_b_signing_key.sign(&agent_b_challenge_bytes);
    let agent_b_signature_hex = hex::encode(agent_b_signature.to_bytes());
    
    let agent_b_confirmation = make_tool_call(&app, &session_id, "confirm_agent", json!({
        "agent_id": agent_b_id,
        "signature": agent_b_signature_hex
    }), json!(5)).await?;
    
    assert!(agent_b_confirmation["result"]["status"].as_str().unwrap() == "confirmed");
    println!("Agent B registered and confirmed");

    let suggestions_response = make_tool_call(&app, &session_id, "get_suggestions", json!({
        "agent_id": agent_b_id,
        "domain": "machine_learning",
        "limit": 10
    }), json!(6)).await?;
    
    let suggestions = suggestions_response["result"]["suggestions"].as_array().unwrap();
    println!("Agent B got {} suggestions", suggestions.len());
    
    let bounty_found = suggestions.iter().any(|suggestion| {
        suggestion["atom_type"].as_str().unwrap_or("") == "bounty" && 
        suggestion["domain"].as_str().unwrap_or("") == "machine_learning" &&
        suggestion["statement"].as_str().unwrap_or("").contains("image classification")
    });
    assert!(bounty_found, "Bounty should appear in Agent B's suggestions");
    println!("Bounty found in suggestions");

    // Step 3: Agent B publishes finding
    println!("\n=== Step 3: Agent B publishes finding ===");
    
    let finding_atom_sig: Vec<u8> = vec![0xCD; 64];
    
    let finding_atoms = json!([{
        "atom_type": "finding",
        "domain": "machine_learning",
        "statement": "Achieved 92% accuracy on medical image classification using attention mechanisms",
        "conditions": {
            "task": "image_classification",
            "dataset_type": "medical",
            "model_type": "cnn_with_attention",
            "accuracy": 0.92,
            "baseline_accuracy": 0.87
        },
        "metrics": {
            "accuracy": 0.92,
            "precision": 0.91,
            "recall": 0.90,
            "f1_score": 0.905
        },
        "provenance": {
            "parent_ids": [],
            "code_hash": "hash123",
            "environment": "pytorch_1.9",
            "dataset_fingerprint": "medical_img_v2",
            "experiment_ref": "exp_001",
            "method_description": "CNN with attention mechanism"
        },
        "signature": finding_atom_sig
    }]);
    
    let finding_top_sig = sign_publish_atoms(&agent_b_signing_key, agent_b_id, &finding_atoms);
    
    let finding_response = make_tool_call(&app, &session_id, "publish_atoms", json!({
        "agent_id": agent_b_id,
        "signature": finding_top_sig,
        "atoms": finding_atoms
    }), json!(7)).await?;
    
    println!("Finding response: {}", serde_json::to_string_pretty(&finding_response).unwrap());
    assert!(finding_response["result"]["published_atoms"].as_array().unwrap().len() == 1);
    println!("Agent B published finding");

    // Step 4: Agent A publishes contradicting finding
    println!("\n=== Step 4: Agent A publishes contradicting finding ===");
    
    tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;
    
    let contradiction_atom_sig: Vec<u8> = vec![0xEF; 64];
    
    let contradiction_atoms = json!([{
        "atom_type": "finding",
        "domain": "machine_learning",
        "statement": "Attention mechanisms do not significantly improve medical image classification accuracy",
        "conditions": {
            "task": "image_classification",
            "dataset_type": "medical",
            "model_type": "cnn_with_attention",
            "accuracy": 0.88,
            "baseline_accuracy": 0.87
        },
        "metrics": {
            "accuracy": 0.88,
            "precision": 0.87,
            "recall": 0.89,
            "f1_score": 0.88
        },
        "provenance": {
            "parent_ids": [],
            "code_hash": "hash456",
            "environment": "tensorflow_2.6",
            "dataset_fingerprint": "medical_img_v2",
            "experiment_ref": "exp_002",
            "method_description": "CNN with attention - different implementation"
        },
        "signature": contradiction_atom_sig
    }]);
    
    let contradiction_top_sig = sign_publish_atoms(&agent_a_signing_key, agent_a_id, &contradiction_atoms);
    
    let contradiction_response = make_tool_call(&app, &session_id, "publish_atoms", json!({
        "agent_id": agent_a_id,
        "signature": contradiction_top_sig,
        "atoms": contradiction_atoms
    }), json!(8)).await?;
    
    assert!(contradiction_response["result"]["published_atoms"].as_array().unwrap().len() == 1);
    println!("Agent A published contradicting finding");

    // Step 5: Verify get_field_map returns synthesis atoms
    println!("\n=== Step 5: Verify get_field_map ===");
    
    let synthesis_atom_sig: Vec<u8> = vec![0x11; 64];
    
    let synthesis_atoms = json!([{
        "atom_type": "synthesis",
        "domain": "machine_learning",
        "statement": "Attention mechanisms show mixed results on medical image classification",
        "conditions": {
            "synthesis_type": "contradiction_resolution",
            "conflicting_findings": 2,
            "consensus_strength": 0.6
        },
        "metrics": {
            "synthesis_confidence": 0.75,
            "evidence_strength": 0.8
        },
        "provenance": {
            "parent_ids": ["finding1", "finding2"],
            "code_hash": "synthesis_hash",
            "environment": null,
            "dataset_fingerprint": null,
            "experiment_ref": null,
            "method_description": "Meta-analysis of conflicting findings"
        },
        "signature": synthesis_atom_sig
    }]);
    
    let synthesis_top_sig = sign_publish_atoms(&agent_a_signing_key, agent_a_id, &synthesis_atoms);
    
    let synthesis_response = make_tool_call(&app, &session_id, "publish_atoms", json!({
        "agent_id": agent_a_id,
        "signature": synthesis_top_sig,
        "atoms": synthesis_atoms
    }), json!(9)).await?;
    
    assert!(synthesis_response["result"]["published_atoms"].as_array().unwrap().len() == 1);
    println!("Agent A published synthesis atom");

    // Test get_field_map
    let field_map_response = make_tool_call(&app, &session_id, "get_field_map", json!({
        "domain": "machine_learning"
    }), json!(10)).await?;
    
    let synthesis_atoms_result = field_map_response["result"]["atoms"].as_array().unwrap();
    println!("Found {} synthesis atoms in machine_learning domain", synthesis_atoms_result.len());
    assert!(!synthesis_atoms_result.is_empty(), "Should find synthesis atoms");
    println!("get_field_map returns synthesis atoms");

    // Step 6: Verify updated suggestions show contradictions
    println!("\n=== Step 6: Verify updated suggestions ===");
    
    tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;
    
    let updated_suggestions = make_tool_call(&app, &session_id, "get_suggestions", json!({
        "agent_id": agent_b_id,
        "domain": "machine_learning",
        "limit": 10
    }), json!(11)).await?;
    
    let updated_suggestions_list = updated_suggestions["result"]["suggestions"].as_array().unwrap();
    println!("Agent B got {} updated suggestions", updated_suggestions_list.len());
    
    assert!(updated_suggestions_list.len() >= 3, "Should have bounty, findings, and synthesis");
    println!("Multiple atoms now available in suggestions");

    // Step 7: Verify search_atoms composability
    println!("\n=== Step 7: Verify search_atoms composability ===");
    
    let search_response = make_tool_call(&app, &session_id, "search_atoms", json!({
        "domain": "machine_learning",
        "type": "finding",
        "text_search": "attention",
        "limit": 5
    }), json!(12)).await?;
    
    let search_results = search_response["result"]["atoms"].as_array().unwrap();
    println!("Search found {} atoms matching criteria", search_results.len());
    
    let attention_findings = search_results.iter().filter(|atom| {
        atom["statement"].as_str().unwrap_or("").contains("attention")
    }).count();
    
    assert!(attention_findings >= 1, "Should find attention-related findings");
    println!("search_atoms composability working");

    // Final validation
    println!("\n=== Final Validation ===");
    
    let total_atoms_response = make_tool_call(&app, &session_id, "search_atoms", json!({
        "limit": 100
    }), json!(13)).await?;
    
    let total_atoms = total_atoms_response["result"]["atoms"].as_array().unwrap().len();
    println!("Total atoms created: {}", total_atoms);
    
    assert!(total_atoms >= 4, "Should have at least 4 atoms (bounty, 2 findings, synthesis)");
    
    println!("End-to-end coordination test PASSED");
    
    Ok(())
}
