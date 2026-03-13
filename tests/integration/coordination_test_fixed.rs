use super::{setup_test_app, make_mcp_request};
use ed25519_dalek::Signer;
use serde_json::json;

#[tokio::test]
async fn test_end_to_end_coordination() -> Result<(), Box<dyn std::error::Error>> {
    let app = setup_test_app().await;

    // Step 1: Agent A registers and publishes bounty
    println!("=== Step 1: Agent A registers and publishes bounty ===");
    
    // Generate Agent A keypair
    let agent_a_signing_key = ed25519_dalek::SigningKey::from_bytes(&[1u8; 32]);
    let agent_a_public_key = hex::encode(agent_a_signing_key.verifying_key().as_bytes());
    
    // Register Agent A
    let registration_response = make_mcp_request(&app, "register_agent", Some(json!({
        "public_key": agent_a_public_key
    })), Some(json!(1))).await?;
    
    assert!(registration_response["result"].is_object());
    let agent_a_id = registration_response["result"]["agent_id"].as_str().unwrap();
    let agent_a_challenge = registration_response["result"]["challenge"].as_str().unwrap();
    println!("Agent A registered with ID: {}", agent_a_id);
    
    // Confirm Agent A (generate actual signature)
    let agent_a_challenge_bytes = hex::decode(agent_a_challenge)?;
    let agent_a_signature = agent_a_signing_key.sign(&agent_a_challenge_bytes);
    let agent_a_signature_hex = hex::encode(agent_a_signature.to_bytes());
    
    let confirmation_response = make_mcp_request(&app, "confirm_agent", Some(json!({
        "agent_id": agent_a_id,
        "signature": agent_a_signature_hex
    })), Some(json!(2))).await?;
    
    println!("Confirmation response: {}", serde_json::to_string_pretty(&confirmation_response).unwrap());
    
    assert!(confirmation_response["result"]["status"].as_str().unwrap() == "confirmed");
    println!("Agent A confirmed");

    // Agent A publishes bounty
    // Generate signature for the individual atom (use valid 128-char hex signature)
    let atom_signature_hex = "abcdef1234567890abcdef1234567890abcdef1234567890abcdef1234567890abcdef1234567890abcdef1234567890abcdef1234567890abcdef1234567890";
    
    // Convert hex string to byte array for JSON
    let atom_signature_bytes: Vec<u8> = (0..atom_signature_hex.len())
        .step_by(2)
        .map(|i| u8::from_str_radix(&atom_signature_hex[i..i+2], 16).unwrap())
        .collect();
    
    // Create the full request with atom signature first
    let bounty_request_with_atom_sig = json!({
        "agent_id": agent_a_id,
        "atoms": [{
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
            "signature": atom_signature_bytes
        }]
    });
    
    // Generate top-level signature against the canonical JSON that includes atom signature
    let canonical_bounty_params = serde_json::to_string(&bounty_request_with_atom_sig)?;
    let bounty_signature = agent_a_signing_key.sign(canonical_bounty_params.as_bytes());
    let bounty_signature_hex = hex::encode(bounty_signature.to_bytes());
    
    println!("Generated top-level signature: {} (length: {})", bounty_signature_hex, bounty_signature_hex.len());
    println!("Generated atom signature: {} (length: {})", atom_signature_hex, atom_signature_hex.len());
    
    let bounty_request = json!({
        "agent_id": agent_a_id,
        "signature": bounty_signature_hex,
        "atoms": [{
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
            "signature": atom_signature_bytes
        }]
    });
    
    println!("Bounty request: {}", serde_json::to_string_pretty(&bounty_request).unwrap());
    
    let bounty_response = make_mcp_request(&app, "publish_atoms", Some(bounty_request), Some(json!(3))).await?;
    
    println!("Bounty response: {}", serde_json::to_string_pretty(&bounty_response).unwrap());
    
    assert!(bounty_response["result"]["published_atoms"].as_array().unwrap().len() == 1);
    println!("Agent A published bounty");

    // Step 2: Agent B registers and calls get_suggestions
    println!("\n=== Step 2: Agent B registers and gets suggestions ===");
    
    // Generate Agent B keypair
    let agent_b_signing_key = ed25519_dalek::SigningKey::from_bytes(&[2u8; 32]);
    let agent_b_public_key = hex::encode(agent_b_signing_key.verifying_key().as_bytes());
    
    // Register Agent B
    let agent_b_registration = make_mcp_request(&app, "register_agent", Some(json!({
        "public_key": agent_b_public_key
    })), Some(json!(4))).await?;
    
    let agent_b_id = agent_b_registration["result"]["agent_id"].as_str().unwrap();
    let agent_b_challenge = agent_b_registration["result"]["challenge"].as_str().unwrap();
    
    // Confirm Agent B (generate actual signature)
    let agent_b_challenge_bytes = hex::decode(agent_b_challenge)?;
    let agent_b_signature = agent_b_signing_key.sign(&agent_b_challenge_bytes);
    let agent_b_signature_hex = hex::encode(agent_b_signature.to_bytes());
    
    let agent_b_confirmation = make_mcp_request(&app, "confirm_agent", Some(json!({
        "agent_id": agent_b_id,
        "signature": agent_b_signature_hex
    })), Some(json!(5))).await?;
    
    assert!(agent_b_confirmation["result"]["status"].as_str().unwrap() == "confirmed");
    println!("Agent B registered and confirmed");

    // Agent B calls get_suggestions and should see the bounty
    let suggestions_response = make_mcp_request(&app, "get_suggestions", Some(json!({
        "agent_id": agent_b_id,
        "domain": "machine_learning",
        "limit": 10
    })), Some(json!(6))).await?;
    
    let suggestions = suggestions_response["result"]["suggestions"].as_array().unwrap();
    println!("Agent B got {} suggestions", suggestions.len());
    
    // Verify bounty appears in suggestions
    let bounty_found = suggestions.iter().any(|suggestion| {
        suggestion["atom_type"].as_str().unwrap() == "bounty" && 
        suggestion["domain"].as_str().unwrap() == "machine_learning" &&
        suggestion["statement"].as_str().unwrap().contains("image classification")
    });
    assert!(bounty_found, "Bounty should appear in Agent B's suggestions");
    println!("✓ Bounty found in suggestions");

    // Step 3: Agent B publishes finding
    println!("\n=== Step 3: Agent B publishes finding ===");
    
    // Generate signature for the individual finding atom
    let finding_atom_signature_hex = "1234567890abcdef1234567890abcdef1234567890abcdef1234567890abcdef1234567890abcdef1234567890abcdef1234567890abcdef1234567890abcdef";
    
    // Convert hex string to byte array for JSON
    let finding_atom_signature_bytes: Vec<u8> = (0..finding_atom_signature_hex.len())
        .step_by(2)
        .map(|i| u8::from_str_radix(&finding_atom_signature_hex[i..i+2], 16).unwrap())
        .collect();
    
    // Create the full request with atom signature first
    let finding_request_with_atom_sig = json!({
        "agent_id": agent_b_id,
        "atoms": [{
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
            "signature": finding_atom_signature_bytes
        }]
    });
    
    // Generate top-level signature against the canonical JSON that includes atom signature
    let finding_canonical_params = serde_json::to_string(&finding_request_with_atom_sig)?;
    let finding_top_level_signature = agent_b_signing_key.sign(finding_canonical_params.as_bytes());
    let finding_top_level_signature_hex = hex::encode(finding_top_level_signature.to_bytes());
    
    let finding_response = make_mcp_request(&app, "publish_atoms", Some(json!({
        "agent_id": agent_b_id,
        "signature": finding_top_level_signature_hex,
        "atoms": finding_request_with_atom_sig["atoms"]
    })), Some(json!(7))).await?;
    
    println!("Finding response: {}", serde_json::to_string_pretty(&finding_response).unwrap());
    assert!(finding_response["result"]["published_atoms"].as_array().unwrap().len() == 1);
    println!("Agent B published finding");

    // Step 4: Agent A publishes contradicting finding
    println!("\n=== Step 4: Agent A publishes contradicting finding ===");
    
    // Add small delay to avoid rate limiting
    tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;
    
    // Generate signature for the contradiction atom
    let contradiction_atom_signature_hex = "fedcba0987654321fedcba0987654321fedcba0987654321fedcba0987654321fedcba0987654321fedcba0987654321fedcba0987654321fedcba0987654321";
    
    // Convert hex string to byte array for JSON
    let contradiction_atom_signature_bytes: Vec<u8> = (0..contradiction_atom_signature_hex.len())
        .step_by(2)
        .map(|i| u8::from_str_radix(&contradiction_atom_signature_hex[i..i+2], 16).unwrap())
        .collect();
    
    // Create the full request with atom signature first
    let contradiction_request_with_atom_sig = json!({
        "agent_id": agent_a_id,
        "atoms": [{
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
            "signature": contradiction_atom_signature_bytes
        }]
    });
    
    // Generate top-level signature against the canonical JSON that includes atom signature
    let contradiction_canonical_params = serde_json::to_string(&contradiction_request_with_atom_sig)?;
    let contradiction_top_level_signature = agent_a_signing_key.sign(contradiction_canonical_params.as_bytes());
    let contradiction_top_level_signature_hex = hex::encode(contradiction_top_level_signature.to_bytes());
    
    let contradiction_response = make_mcp_request(&app, "publish_atoms", Some(json!({
        "agent_id": agent_a_id,
        "signature": contradiction_top_level_signature_hex,
        "atoms": contradiction_request_with_atom_sig["atoms"]
    })), Some(json!(8))).await?;
    
    assert!(contradiction_response["result"]["published_atoms"].as_array().unwrap().len() == 1);
    println!("Agent A published contradicting finding");

    // Step 5: Verify get_field_map returns synthesis atoms
    println!("\n=== Step 5: Verify get_field_map ===");
    
    // Generate signature for the synthesis atom
    let synthesis_atom_signature_hex = "111111112222222233333333444444445555555566666666777777778888888899999999000000001111111122222222";
    
    // Convert hex string to byte array for JSON
    let synthesis_atom_signature_bytes: Vec<u8> = (0..synthesis_atom_signature_hex.len())
        .step_by(2)
        .map(|i| u8::from_str_radix(&synthesis_atom_signature_hex[i..i+2], 16).unwrap())
        .collect();
    
    // Create the full request with atom signature first
    let synthesis_request_with_atom_sig = json!({
        "agent_id": agent_a_id,
        "atoms": [{
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
            "signature": synthesis_atom_signature_bytes
        }]
    });
    
    // Generate top-level signature against the canonical JSON that includes atom signature
    let synthesis_canonical_params = serde_json::to_string(&synthesis_request_with_atom_sig)?;
    let synthesis_top_level_signature = agent_a_signing_key.sign(synthesis_canonical_params.as_bytes());
    let synthesis_top_level_signature_hex = hex::encode(synthesis_top_level_signature.to_bytes());
    
    let synthesis_response = make_mcp_request(&app, "publish_atoms", Some(json!({
        "agent_id": agent_a_id,
        "signature": synthesis_top_level_signature_hex,
        "atoms": synthesis_request_with_atom_sig["atoms"]
    })), Some(json!(9))).await?;
    
    assert!(synthesis_response["result"]["published_atoms"].as_array().unwrap().len() == 1);
    println!("Agent A published synthesis atom");

    // Test get_field_map
    let field_map_response = make_mcp_request(&app, "get_field_map", Some(json!({
        "domain": "machine_learning"
    })), Some(json!(10))).await?;
    
    let synthesis_atoms = field_map_response["result"]["atoms"].as_array().unwrap();
    println!("Found {} synthesis atoms in machine_learning domain", synthesis_atoms.len());
    assert!(!synthesis_atoms.is_empty(), "Should find synthesis atoms");
    println!("✓ get_field_map returns synthesis atoms");

    // Step 6: Verify updated suggestions show contradictions
    println!("\n=== Step 6: Verify updated suggestions ===");
    
    // Wait a moment for processing
    tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;
    
    let updated_suggestions = make_mcp_request(&app, "get_suggestions", Some(json!({
        "agent_id": agent_b_id,
        "domain": "machine_learning",
        "limit": 10
    })), Some(json!(11))).await?;
    
    let updated_suggestions_list = updated_suggestions["result"]["suggestions"].as_array().unwrap();
    println!("Agent B got {} updated suggestions", updated_suggestions_list.len());
    
    // Check that we have multiple atoms now
    assert!(updated_suggestions_list.len() >= 3, "Should have bounty, findings, and synthesis");
    println!("✓ Multiple atoms now available in suggestions");

    // Step 7: Verify search_atoms composability
    println!("\n=== Step 7: Verify search_atoms composability ===");
    
    // Test search with multiple filters
    let search_response = make_mcp_request(&app, "search_atoms", Some(json!({
        "domain": "machine_learning",
        "type": "finding",
        "text_search": "attention",
        "limit": 5
    })), Some(json!(12))).await?;
    
    let search_results = search_response["result"]["atoms"].as_array().unwrap();
    println!("Search found {} atoms matching criteria", search_results.len());
    
    // Should find the attention-related findings
    let attention_findings = search_results.iter().filter(|atom| {
        atom["statement"].as_str().unwrap().contains("attention")
    }).count();
    
    assert!(attention_findings >= 1, "Should find attention-related findings");
    println!("✓ search_atoms composability working");

    // Final validation
    println!("\n=== Final Validation ===");
    
    // Count total atoms via search
    let total_atoms_response = make_mcp_request(&app, "search_atoms", Some(json!({
        "limit": 100
    })), Some(json!(13))).await?;
    
    let total_atoms = total_atoms_response["result"]["atoms"].as_array().unwrap().len();
    println!("Total atoms created: {}", total_atoms);
    
    // Verify coordination system is working
    assert!(total_atoms >= 4, "Should have at least 4 atoms (bounty, 2 findings, synthesis)");
    
    println!("✅ End-to-end coordination test PASSED");
    println!("🎯 Mote coordination system is functioning correctly!");
    
    Ok(())
}
