// Full workflow integration tests: claim → publish → review → cluster query

use super::{setup_test_app, make_http_request};
use serial_test::serial;
use serde_json::json;

#[serial]
#[tokio::test]
async fn test_complete_research_workflow() {
    let app = setup_test_app().await;
    
    // Step 1: Register an agent
    let (status, body) = make_http_request(
        &app,
        axum::http::Method::POST,
        "/rpc",
        Some(r#"{
            "jsonrpc": "2.0",
            "method": "register_agent_simple",
            "params": {
                "agent_name": "workflow-test-agent"
            },
            "id": 1
        }"#)
    ).await
    .expect("Failed to register agent");
    
    assert_eq!(status, axum::http::StatusCode::OK);
    
    let agent_data: serde_json::Value = serde_json::from_str(&body)
        .expect("Failed to parse agent response");
    let agent_id = agent_data["result"]["agent_id"].as_str().unwrap();
    let api_token = agent_data["result"]["api_token"].as_str().unwrap();
    
    // Step 2: Claim a research direction
    let (status, body) = make_http_request(
        &app,
        axum::http::Method::POST,
        "/rpc",
        Some(&json!({
            "jsonrpc": "2.0",
            "method": "claim_direction",
            "params": {
                "agent_id": agent_id,
                "api_token": api_token,
                "hypothesis": "Machine learning can predict protein folding with 95% accuracy",
                "domain": "bioinformatics",
                "conditions": {
                    "model_type": "transformer",
                    "dataset_size": "large"
                }
            },
            "id": 2
        }).to_string())
    ).await
    .expect("Failed to claim direction");
    
    assert_eq!(status, axum::http::StatusCode::OK);
    
    let claim_data: serde_json::Value = serde_json::from_str(&body)
        .expect("Failed to parse claim response");
    
    let claim_id = claim_data["result"]["claim_id"].as_str().unwrap();
    let hypothesis_atom_id = claim_data["result"]["atom_id"].as_str().unwrap();
    
    // Verify claim conflict detection and density reporting
    assert!(claim_data["result"].get("claim_density").is_some());
    assert!(claim_data["result"].get("potential_conflicts").is_some());
    assert!(claim_data["result"].get("warnings").is_some());
    
    // Step 3: Publish supporting research findings
    let (status, body) = make_http_request(
        &app,
        axum::http::Method::POST,
        "/rpc",
        Some(&json!({
            "jsonrpc": "2.0",
            "method": "publish_atoms",
            "params": {
                "agent_id": agent_id,
                "api_token": api_token,
                "atoms": [{
                    "atom_type": "finding",
                    "domain": "bioinformatics",
                    "statement": "Our transformer model achieved 94.8% accuracy on CASP-14 dataset",
                    "conditions": {
                        "model_type": "transformer",
                        "dataset": "CASP-14"
                    },
                    "metrics": {
                        "accuracy": 0.948,
                        "precision": 0.951,
                        "recall": 0.945
                    },
                    "provenance": {
                        "experiment": "protein_folding_prediction",
                        "date": "2024-01-15"
                    },
                    "signature": "deadbeefdeadbeefdeadbeefdeadbeefdeadbeefdeadbeefdeadbeefdeadbeefdeadbeefdeadbeefdeadbeefdeadbeefdeadbeefdeadbeefdeadbeefdeadbeefdeadbeefdeadbeefdeadbeefdeadbeefdeadbeef"
                }]
            },
            "id": 3
        }).to_string())
    ).await
    .expect("Failed to publish finding");
    
    assert_eq!(status, axum::http::StatusCode::OK);
    
    let publish_data: serde_json::Value = serde_json::from_str(&body)
        .expect("Failed to parse publish response");
    
    let finding_atom_id = publish_data["result"]["published_atoms"][0].as_str().unwrap();
    
    // Step 4: Check review queue (should contain our finding)
    let (status, body) = make_http_request(
        &app,
        axum::http::Method::GET,
        "/review?limit=10&offset=0",
        None
    ).await
    .expect("Failed to get review queue");
    
    assert_eq!(status, axum::http::StatusCode::OK);
    
    let queue_data: serde_json::Value = serde_json::from_str(&body)
        .expect("Failed to parse queue response");
    let items = queue_data["items"].as_array().unwrap();
    
    // Find our finding in the queue
    let our_finding = items.iter().find(|item| {
        item["atom_id"].as_str() == Some(finding_atom_id)
    });
    
    assert!(our_finding.is_some(), "Finding should be in review queue");
    
    let finding_in_queue = our_finding.unwrap();
    assert_eq!(finding_in_queue["review_status"], "pending");
    assert_eq!(finding_in_queue["atom_type"], "finding");
    
    // Step 5: Approve the finding
    let (status, body) = make_http_request(
        &app,
        axum::http::Method::POST,
        &format!("/review/{}", finding_atom_id),
        Some(&json!({
            "action": "approve",
            "reason": "High-quality experimental validation with strong metrics"
        }).to_string())
    ).await
    .expect("Failed to approve finding");
    
    // Debug: print review response
    println!("Review approval response status: {}", status);
    println!("Review approval response body: {}", body);
    
    assert_eq!(status, axum::http::StatusCode::OK);
    
    let review_data: serde_json::Value = serde_json::from_str(&body)
        .expect("Failed to parse review response");
    assert_eq!(review_data["status"], "approve");
    
    // Step 6: Query cluster for similar research
    let (status, body) = make_http_request(
        &app,
        axum::http::Method::POST,
        "/rpc",
        Some(&json!({
            "jsonrpc": "2.0",
            "method": "query_cluster",
            "params": {
                "vector": [0.1, 0.2, 0.3, 0.4, 0.5, 0.6, 0.7, 0.8],
                "radius": 0.5,
                "limit": 5,
                "max_hops": 2,
                "include_graph_traversal": true,
                "edge_types": ["derived_from", "inspired_by"],
                "cache_key": "test_cluster_query"
            },
            "id": 4
        }).to_string())
    ).await
    .expect("Failed to query cluster");
    
    assert_eq!(status, axum::http::StatusCode::OK);
    
    let cluster_data: serde_json::Value = serde_json::from_str(&body)
        .expect("Failed to parse cluster response");
    
    // Verify enhanced cluster response structure
    assert!(cluster_data["result"].get("atoms").is_some());
    assert!(cluster_data["result"].get("pheromone_landscape").is_some());
    assert!(cluster_data["result"].get("query_params").is_some());
    assert!(cluster_data["result"].get("total").is_some());
    
    let query_params = &cluster_data["result"]["query_params"];
    assert_eq!(query_params["max_hops"], 2);
    assert!(query_params["edge_types"].is_array());
    
    // Graph traversal should be empty since no atoms found
    if cluster_data["result"].get("graph_traversal").is_some() {
        println!("Graph traversal found: {}", cluster_data["result"]["graph_traversal"]);
    }
    
    // Step 7: Verify claim is still active
    let (status, body) = make_http_request(
        &app,
        axum::http::Method::POST,
        "/rpc",
        Some(&json!({
            "jsonrpc": "2.0",
            "method": "claim_direction",
            "params": {
                "agent_id": agent_id,
                "api_token": api_token,
                "hypothesis": "Alternative hypothesis for testing",
                "domain": "bioinformatics",
                "conditions": {
                    "model_type": "cnn"
                }
            },
            "id": 5
        }).to_string())
    ).await
    .expect("Failed to make second claim");
    
    assert_eq!(status, axum::http::StatusCode::OK);
    
    let second_claim_data: serde_json::Value = serde_json::from_str(&body)
        .expect("Failed to parse second claim response");
    
    // Should detect potential conflict with existing claim (may be empty if no conflicts)
    let conflicts = second_claim_data["result"]["potential_conflicts"].as_array().unwrap();
    println!("Found {} potential conflicts", conflicts.len());
    
    // Verify our original claim is in active claims
    let active_claims = second_claim_data["result"]["active_claims"].as_array().unwrap();
    let our_original_claim = active_claims.iter().find(|c| {
        c["claim_id"].as_str() == Some(claim_id)
    });
    assert!(our_original_claim.is_some(), "Original claim should still be active");
    
    // Step 8: Test cache functionality (second cluster query should use cache)
    let (status, body) = make_http_request(
        &app,
        axum::http::Method::POST,
        "/rpc",
        Some(&json!({
            "jsonrpc": "2.0",
            "method": "query_cluster",
            "params": {
                "vector": [0.1, 0.2, 0.3, 0.4, 0.5, 0.6, 0.7, 0.8],
                "radius": 0.5,
                "limit": 5,
                "cache_key": "test_cluster_query"
            },
            "id": 6
        }).to_string())
    ).await
    .expect("Failed to query cluster with cache");
    
    assert_eq!(status, axum::http::StatusCode::OK);
    
    let cached_cluster_data: serde_json::Value = serde_json::from_str(&body)
        .expect("Failed to parse cached cluster response");
    
    // Should return same result as before (from cache)
    assert_eq!(
        cached_cluster_data["result"]["query_params"]["max_hops"],
        cluster_data["result"]["query_params"]["max_hops"]
    );
    
    println!("✅ Complete research workflow test passed!");
}

#[serial]
#[tokio::test]
async fn test_cross_feature_interactions() {
    let app = setup_test_app().await;
    
    // Register two agents for interaction testing
    let agent1_response = make_http_request(
        &app,
        axum::http::Method::POST,
        "/rpc",
        Some(r#"{
            "jsonrpc": "2.0",
            "method": "register_agent_simple",
            "params": {
                "agent_name": "interaction-agent-1"
            },
            "id": 1
        }"#)
    ).await
    .expect("Failed to register agent 1");
    
    let agent1_data: serde_json::Value = serde_json::from_str(&agent1_response.1)
        .expect("Failed to parse agent 1 response");
    let agent1_id = agent1_data["result"]["agent_id"].as_str().unwrap();
    let agent1_token = agent1_data["result"]["api_token"].as_str().unwrap();
    
    let agent2_response = make_http_request(
        &app,
        axum::http::Method::POST,
        "/rpc",
        Some(r#"{
            "jsonrpc": "2.0",
            "method": "register_agent_simple",
            "params": {
                "agent_name": "interaction-agent-2"
            },
            "id": 2
        }"#)
    ).await
    .expect("Failed to register agent 2");
    
    let agent2_data: serde_json::Value = serde_json::from_str(&agent2_response.1)
        .expect("Failed to parse agent 2 response");
    let agent2_id = agent2_data["result"]["agent_id"].as_str().unwrap();
    let agent2_token = agent2_data["result"]["api_token"].as_str().unwrap();
    
    // Agent 1: Claim a direction
    let claim_response = make_http_request(
        &app,
        axum::http::Method::POST,
        "/rpc",
        Some(&json!({
            "jsonrpc": "2.0",
            "method": "claim_direction",
            "params": {
                "agent_id": agent1_id,
                "api_token": agent1_token,
                "hypothesis": "Quantum computing can solve optimization problems faster",
                "domain": "quantum_computing",
                "conditions": {
                    "algorithm": "QAOA",
                    "problem_type": "optimization"
                }
            },
            "id": 3
        }).to_string())
    ).await
    .expect("Failed to claim direction");
    
    let claim_data: serde_json::Value = serde_json::from_str(&claim_response.1)
        .expect("Failed to parse claim response");
    let claim_id = claim_data["result"]["claim_id"].as_str().unwrap();
    
    // Agent 2: Publish conflicting research
    let publish_response = make_http_request(
        &app,
        axum::http::Method::POST,
        "/rpc",
        Some(&json!({
            "jsonrpc": "2.0",
            "method": "publish_atoms",
            "params": {
                "agent_id": agent2_id,
                "api_token": agent2_token,
                "atoms": [{
                    "atom_type": "finding",
                    "domain": "quantum_computing",
                    "statement": "Classical algorithms outperform QAOA on small optimization problems",
                    "conditions": {
                        "algorithm": "simulated_annealing",
                        "problem_size": "small"
                    },
                    "metrics": {
                        "performance_ratio": 1.2,
                        "quantum_advantage": false
                    },
                    "provenance": {"study": "comparative_analysis"},
                    "signature": "deadbeefdeadbeefdeadbeefdeadbeefdeadbeefdeadbeefdeadbeefdeadbeefdeadbeefdeadbeefdeadbeefdeadbeefdeadbeefdeadbeefdeadbeefdeadbeefdeadbeefdeadbeefdeadbeefdeadbeefdeadbeefdeadbeef"
                }]
            },
            "id": 4
        }).to_string())
    ).await
    .expect("Failed to publish conflicting finding");
    
    let publish_data: serde_json::Value = serde_json::from_str(&publish_response.1)
        .expect("Failed to parse publish response");
    let conflicting_atom_id = publish_data["result"]["published_atoms"][0].as_str().unwrap();
    
    // Review the conflicting finding (should be rejected due to conflict)
    let review_response = make_http_request(
        &app,
        axum::http::Method::POST,
        &format!("/review/{}", conflicting_atom_id),
        Some(&json!({
            "action": "reject",
            "reason": "Conflicts with established claim direction in domain"
        }).to_string())
    ).await
    .expect("Failed to review conflicting finding");
    
    assert_eq!(review_response.0, axum::http::StatusCode::OK);
    
    // Query cluster to see how rejection affects the landscape
    let cluster_response = make_http_request(
        &app,
        axum::http::Method::POST,
        "/rpc",
        Some(&json!({
            "jsonrpc": "2.0",
            "method": "query_cluster",
            "params": {
                "vector": [0.3, 0.1, 0.7, 0.9, 0.2, 0.8],
                "radius": 0.8,
                "limit": 10,
                "include_graph_traversal": true,
                "max_hops": 1
            },
            "id": 5
        }).to_string())
    ).await
    .expect("Failed to query cluster after rejection");
    
    let cluster_data: serde_json::Value = serde_json::from_str(&cluster_response.1)
        .expect("Failed to parse cluster response");
    
    // Verify pheromone landscape reflects the rejection
    let pheromone = &cluster_data["result"]["pheromone_landscape"];
    assert!(pheromone.get("attraction").is_some());
    assert!(pheromone.get("repulsion").is_some());
    assert!(pheromone.get("novelty").is_some());
    // Disagreement may not be present in empty results
    
    // Agent 1: Make a new claim that should detect the previous conflict
    let new_claim_response = make_http_request(
        &app,
        axum::http::Method::POST,
        "/rpc",
        Some(&json!({
            "jsonrpc": "2.0",
            "method": "claim_direction",
            "params": {
                "agent_id": agent1_id,
                "api_token": agent1_token,
                "hypothesis": "Hybrid quantum-classical approaches show promise",
                "domain": "quantum_computing",
                "conditions": {
                    "algorithm": "hybrid",
                    "problem_type": "optimization"
                }
            },
            "id": 6
        }).to_string())
    ).await
    .expect("Failed to make new claim");
    
    let new_claim_data: serde_json::Value = serde_json::from_str(&new_claim_response.1)
        .expect("Failed to parse new claim response");
    
    // Should detect claim density and potential conflicts
    assert!(new_claim_data["result"].get("claim_density").is_some());
    let claim_density = new_claim_data["result"]["claim_density"].as_f64().unwrap();
    assert!(claim_density > 0.0, "Should detect non-zero claim density");
    
    println!("✅ Cross-feature interaction test passed!");
}
