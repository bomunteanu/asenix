use reqwest;
use serde_json::json;
use std::path::PathBuf;
use tempfile::TempDir;
use tokio::process::Command;

// Test helper to start a test server
async fn start_test_server() -> (String, TempDir) {
    let temp_dir = TempDir::new().unwrap();
    let storage_path = temp_dir.path().join("artifacts");
    
    // Create test config
    let config_content = format!(
        r#"
[hub]
listen_address = "127.0.0.1:0"
database_url = "postgresql://test"
artifact_storage_path = "{}"
max_blob_size = 1048576
max_storage_per_agent = 10485760

[pheromone]
attraction_decay_rate = 0.95
repulsion_decay_rate = 0.95
novelty_decay_rate = 0.95
disagreement_decay_rate = 0.95

[trust]
initial_trust = 0.5
trust_decay_rate = 0.99

[workers]
embedding_queue_size = 1000
staleness_check_interval = 600

[acceptance]
enable_validation = true
"#,
        storage_path.display()
    );
    
    let config_file = temp_dir.path().join("config.toml");
    std::fs::write(&config_file, config_content).unwrap();
    
    // Start the server in background
    let mut child = Command::new("cargo")
        .args(&["run", "--bin", "mote", "--", "--config", config_file.to_str().unwrap()])
        .spawn()
        .expect("Failed to start server");
    
    // Give server time to start and read port from stdout
    tokio::time::sleep(tokio::time::Duration::from_secs(3)).await;
    
    // For now, use a default port - in a real test we'd parse the server output
    let server_url = "http://127.0.0.1:8080".to_string();
    
    (server_url, temp_dir)
}

#[tokio::test]
async fn test_inline_artifact_upload_via_publish_atoms() {
    // This test demonstrates the unified artifact handling
    // where artifacts are uploaded inline with publish_atoms
    
    let (server_url, _temp_dir) = start_test_server().await;
    let client = reqwest::Client::new();
    
    // Step 1: Register an agent
    let register_response = client
        .post(&format!("{}/rpc", server_url))
        .json(&json!({
            "jsonrpc": "2.0",
            "method": "register_agent_simple",
            "params": {
                "agent_name": "test-artifact-agent"
            },
            "id": 1
        }))
        .send()
        .await
        .expect("Failed to register agent");
    
    let register_result: serde_json::Value = register_response
        .json()
        .await
        .expect("Failed to parse register response");
    
    let agent_id = register_result["result"]["agent_id"].as_str().unwrap();
    let api_token = register_result["result"]["api_token"].as_str().unwrap();
    
    // Step 2: Create an atom with an inline artifact (blob)
    let file_content = b"This is test file content for artifact upload.";
    let base64_content = base64::encode(file_content);
    
    let publish_response = client
        .post(&format!("{}/rpc", server_url))
        .json(&json!({
            "jsonrpc": "2.0",
            "method": "publish_atoms",
            "params": {
                "agent_id": agent_id,
                "api_token": api_token,
                "atoms": [{
                    "atom_type": "Finding",
                    "domain": "test-domain",
                    "statement": "Test finding with inline artifact",
                    "conditions": {"experiment": "test"},
                    "metrics": {"accuracy": 0.95},
                    "provenance": {"experiment_id": "exp123"},
                    "signature": "deadbeefdeadbeefdeadbeefdeadbeefdeadbeefdeadbeefdeadbeefdeadbeefdeadbeef",
                    "artifact_inline": {
                        "artifact_type": "blob",
                        "content": {
                            "data": base64_content
                        },
                        "media_type": "text/plain"
                    }
                }]
            },
            "id": 2
        }))
        .send()
        .await
        .expect("Failed to publish atom with artifact");
    
    let publish_result: serde_json::Value = publish_response
        .json()
        .await
        .expect("Failed to parse publish response");
    
    // Verify the atom was published successfully
    assert!(publish_result["result"].is_array());
    let atoms = publish_result["result"].as_array().unwrap();
    assert_eq!(atoms.len(), 1);
    
    let atom = &atoms[0];
    assert_eq!(atom["domain"], "test-domain");
    assert_eq!(atom["statement"], "Test finding with inline artifact");
    assert!(atom["artifact_tree_hash"].is_string());
    
    let artifact_hash = atom["artifact_tree_hash"].as_str().unwrap();
    assert!(!artifact_hash.is_empty());
    assert_eq!(artifact_hash.len(), 64); // BLAKE3 hash length
    
    // Step 3: Download the artifact using MCP tool
    let download_response = client
        .post(&format!("{}/mcp", server_url))
        .header("Content-Type", "application/json")
        .json(&json!({
            "jsonrpc": "2.0",
            "method": "tools/call",
            "params": {
                "name": "download_artifact",
                "arguments": {
                    "hash": artifact_hash,
                    "encoding": "base64"
                }
            },
            "id": 3
        }))
        .send()
        .await
        .expect("Failed to download artifact");
    
    let download_result: serde_json::Value = download_response
        .json()
        .await
        .expect("Failed to parse download response");
    
    // Verify the downloaded content matches
    let downloaded_content = download_result["result"]["content"].as_str().unwrap();
    let decoded_content = base64::decode(downloaded_content).unwrap();
    assert_eq!(decoded_content, file_content);
    
    // Step 4: Get artifact metadata
    let metadata_response = client
        .post(&format!("{}/mcp", server_url))
        .header("Content-Type", "application/json")
        .json(&json!({
            "jsonrpc": "2.0",
            "method": "tools/call",
            "params": {
                "name": "get_artifact_metadata",
                "arguments": {
                    "hash": artifact_hash
                }
            },
            "id": 4
        }))
        .send()
        .await
        .expect("Failed to get artifact metadata");
    
    let metadata_result: serde_json::Value = metadata_response
        .json()
        .await
        .expect("Failed to parse metadata response");
    
    // Verify metadata
    let metadata = &metadata_result["result"];
    assert_eq!(metadata["hash"], artifact_hash);
    assert_eq!(metadata["type"], "blob");
    assert_eq!(metadata["media_type"], "text/plain");
    assert_eq!(metadata["uploaded_by"], agent_id);
    assert!(metadata["size_bytes"].is_number());
    
    // Step 5: List artifacts
    let list_response = client
        .post(&format!("{}/mcp", server_url))
        .header("Content-Type", "application/json")
        .json(&json!({
            "jsonrpc": "2.0",
            "method": "tools/call",
            "params": {
                "name": "list_artifacts",
                "arguments": {
                    "artifact_type": "blob",
                    "uploaded_by": agent_id
                }
            },
            "id": 5
        }))
        .send()
        .await
        .expect("Failed to list artifacts");
    
    let list_result: serde_json::Value = list_response
        .json()
        .await
        .expect("Failed to parse list response");
    
    // Verify our artifact is in the list
    let artifacts = list_result["result"].as_array().unwrap();
    assert!(!artifacts.is_empty());
    
    let found_artifact = artifacts.iter().find(|a| a["hash"] == artifact_hash);
    assert!(found_artifact.is_some());
    
    println!("✅ Successfully tested inline artifact upload and management!");
}

#[tokio::test] 
async fn test_inline_tree_artifact_upload() {
    // This test demonstrates uploading a tree (folder) artifact inline
    
    let (server_url, _temp_dir) = start_test_server().await;
    let client = reqwest::Client::new();
    
    // Register agent
    let register_response = client
        .post(&format!("{}/rpc", server_url))
        .json(&json!({
            "jsonrpc": "2.0",
            "method": "register_agent_simple",
            "params": {
                "agent_name": "test-tree-agent"
            },
            "id": 1
        }))
        .send()
        .await
        .expect("Failed to register agent");
    
    let register_result: serde_json::Value = register_response.json().await.expect("Failed to parse register response");
    let agent_id = register_result["result"]["agent_id"].as_str().unwrap();
    let api_token = register_result["result"]["api_token"].as_str().unwrap();
    
    // Create a tree artifact (folder structure)
    let tree_entries = json!([
        {
            "name": "paper.pdf",
            "hash": "abc123def456789abc123def456789abc123def456789abc123def456789abc123",
            "type_": "blob"
        },
        {
            "name": "data",
            "hash": "def456abc123def456789abc123def456789abc123def456789abc123def456789",
            "type_": "tree"
        },
        {
            "name": "code/main.py",
            "hash": "789abc123def456789abc123def456789abc123def456789abc123def456789abc",
            "type_": "blob"
        }
    ]);
    
    // Publish atom with inline tree artifact
    let publish_response = client
        .post(&format!("{}/rpc", server_url))
        .json(&json!({
            "jsonrpc": "2.0",
            "method": "publish_atoms",
            "params": {
                "agent_id": agent_id,
                "api_token": api_token,
                "atoms": [{
                    "atom_type": "Hypothesis",
                    "domain": "research",
                    "statement": "Research hypothesis with supporting data package",
                    "conditions": {"experiment": "multi-modal"},
                    "metrics": {"confidence": 0.85},
                    "provenance": {"study_id": "study456"},
                    "signature": "deadbeefdeadbeefdeadbeefdeadbeefdeadbeefdeadbeefdeadbeefdeadbeefdeadbeef",
                    "artifact_inline": {
                        "artifact_type": "tree",
                        "content": {
                            "entries": tree_entries
                        }
                    }
                }]
            },
            "id": 2
        }))
        .send()
        .await
        .expect("Failed to publish atom with tree artifact");
    
    let publish_result: serde_json::Value = publish_response.json().await.expect("Failed to parse publish response");
    
    // Verify the atom was published
    let atoms = publish_result["result"].as_array().unwrap();
    assert_eq!(atoms.len(), 1);
    
    let atom = &atoms[0];
    assert!(atom["artifact_tree_hash"].is_string());
    
    let artifact_hash = atom["artifact_tree_hash"].as_str().unwrap();
    
    // Get metadata to verify it's a tree
    let metadata_response = client
        .post(&format!("{}/mcp", server_url))
        .header("Content-Type", "application/json")
        .json(&json!({
            "jsonrpc": "2.0",
            "method": "tools/call",
            "params": {
                "name": "get_artifact_metadata",
                "arguments": {
                    "hash": artifact_hash
                }
            },
            "id": 3
        }))
        .send()
        .await
        .expect("Failed to get artifact metadata");
    
    let metadata_result: serde_json::Value = metadata_response.json().await.expect("Failed to parse metadata response");
    let metadata = &metadata_result["result"];
    
    assert_eq!(metadata["type"], "tree");
    assert_eq!(metadata["uploaded_by"], agent_id);
    
    println!("✅ Successfully tested inline tree artifact upload!");
}
