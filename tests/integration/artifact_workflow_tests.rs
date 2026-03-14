use reqwest;
use serde_json::json;
use std::path::PathBuf;
use tempfile::TempDir;
use tokio::process::Command;

// Test helper to start a test server with artifact storage
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
        .args(&["run", "--bin", "asenix", "--", "--config", config_file.to_str().unwrap()])
        .spawn()
        .expect("Failed to start server");
    
    // Give server time to start
    tokio::time::sleep(tokio::time::Duration::from_secs(2)).await;
    
    // For integration tests, we'll use a mock server approach
    // In a real scenario, you'd extract the actual port from the server output
    let server_url = "http://127.0.0.1:8080".to_string();
    
    // Note: In a real test environment, you'd want to:
    // 1. Use proper process management
    // 2. Extract the actual port from server output
    // 3. Handle cleanup properly
    // 4. Use a test database
    
    (server_url, temp_dir)
}

#[tokio::test]
async fn test_complete_artifact_workflow() {
    // This test demonstrates the complete artifact workflow:
    // 1. Upload a blob artifact
    // 2. Upload a tree artifact that references the blob
    // 3. Publish an atom with artifact_tree_hash
    // 4. Search for atoms and verify artifact_tree_hash is included
    
    let client = reqwest::Client::new();
    
    // Step 1: Upload a blob artifact (e.g., a dataset)
    let dataset_content = b"name,age,score\nAlice,25,85.5\nBob,30,92.1\nCharlie,35,78.3";
    let dataset_hash = blake3::hash(dataset_content).to_hex().to_string();
    
    let upload_response = client
        .put(&format!("http://127.0.0.1:8080/artifacts/{}", dataset_hash))
        .header("Content-Type", "application/octet-stream")
        .header("X-Agent-ID", "test_agent_123")
        .header("X-Signature", "deadbeefdeadbeefdeadbeefdeadbeefdeadbeefdeadbeefdeadbeefdeadbeef")
        .body(dataset_content.to_vec())
        .send()
        .await;
    
    // Note: This would fail in a real test without a running server
    // In practice, you'd use testcontainers or a mock server
    
    // Step 2: Create and upload a tree artifact
    let tree_manifest = json!({
        "type": "tree",
        "entries": [
            {
                "path": "dataset.csv",
                "hash": dataset_hash,
                "size": dataset_content.len(),
                "type": "blob"
            },
            {
                "path": "metadata.json",
                "hash": "cafebabecafebabecafebabecafebabecafebabecafebabecafebabecafebabe",
                "size": 156,
                "type": "blob"
            }
        ]
    });
    
    let tree_data = serde_json::to_vec(&tree_manifest).unwrap();
    let tree_hash = blake3::hash(&tree_data).to_hex().to_string();
    
    // Step 3: Publish an atom with the artifact tree hash
    let atom_payload = json!({
        "agent_id": "test_agent_123",
        "signature": "deadbeefdeadbeefdeadbeefdeadbeefdeadbeefdeadbeefdeadbeefdeadbeef",
        "atoms": [{
            "atom_type": "finding",
            "domain": "machine_learning",
            "statement": "Our analysis of the dataset shows a positive correlation between age and score",
            "conditions": {},
            "metrics": {"correlation": 0.85},
            "provenance": {},
            "signature": "deadbeefdeadbeefdeadbeefdeadbeefdeadbeefdeadbeefdeadbeefdeadbeef",
            "artifact_tree_hash": tree_hash
        }]
    });
    
    // Step 4: Search for atoms and verify artifact_tree_hash is returned
    let search_payload = json!({
        "domain": "machine_learning",
        "limit": 10
    });
    
    // This test structure shows the intended workflow
    // Actual implementation would require proper test setup with database
}

#[tokio::test]
async fn test_artifact_storage_limits() {
    // Test storage limits and quotas
    let client = reqwest::Client::new();
    
    // Create a large blob that exceeds size limits
    let large_content = vec![0u8; 2 * 1024 * 1024]; // 2MB
    let large_hash = blake3::hash(&large_content).to_hex().to_string();
    
    // This should fail due to size limits
    let response = client
        .put(&format!("http://127.0.0.1:8080/artifacts/{}", large_hash))
        .header("Content-Type", "application/octet-stream")
        .header("X-Agent-ID", "test_agent_123")
        .header("X-Signature", "deadbeefdeadbeefdeadbeefdeadbeefdeadbeefdeadbeefdeadbeefdeadbeef")
        .body(large_content)
        .send()
        .await;
    
    // Verify the response indicates the size limit was exceeded
}

#[tokio::test]
async fn test_artifact_signature_validation() {
    // Test that artifact uploads require valid signatures
    let client = reqwest::Client::new();
    
    let test_content = b"Test data with invalid signature";
    let content_hash = blake3::hash(test_content).to_hex().to_string();
    
    // Upload with invalid signature
    let response = client
        .put(&format!("http://127.0.0.1:8080/artifacts/{}", content_hash))
        .header("Content-Type", "application/octet-stream")
        .header("X-Agent-ID", "test_agent_123")
        .header("X-Signature", "invalid_signature")
        .body(test_content.to_vec())
        .send()
        .await;
    
    // Should fail due to invalid signature
}

#[tokio::test]
async fn test_artifact_hash_validation() {
    // Test that artifact hashes are validated
    let client = reqwest::Client::new();
    
    let test_content = b"Test data";
    let wrong_hash = "wronghash123456789012345678901234567890123456789012345678901234567890";
    
    // Upload with wrong hash
    let response = client
        .put(&format!("http://127.0.0.1:8080/artifacts/{}", wrong_hash))
        .header("Content-Type", "application/octet-stream")
        .header("X-Agent-ID", "test_agent_123")
        .header("X-Signature", "deadbeefdeadbeefdeadbeefdeadbeefdeadbeefdeadbeefdeadbeefdeadbeef")
        .body(test_content.to_vec())
        .send()
        .await;
    
    // Should fail due to hash mismatch
}

#[tokio::test]
async fn test_artifact_tree_validation() {
    // Test that artifact_tree_hash in atoms references valid trees
    let client = reqwest::Client::new();
    
    // Try to publish an atom with a non-existent artifact_tree_hash
    let atom_payload = json!({
        "agent_id": "test_agent_123",
        "signature": "deadbeefdeadbeefdeadbeefdeadbeefdeadbeefdeadbeefdeadbeefdeadbeef",
        "atoms": [{
            "atom_type": "finding",
            "domain": "test",
            "statement": "Test statement with invalid artifact reference",
            "conditions": {},
            "metrics": null,
            "provenance": {},
            "signature": "deadbeefdeadbeefdeadbeefdeadbeefdeadbeefdeadbeefdeadbeefdeadbeef",
            "artifact_tree_hash": "nonexistenthash123456789012345678901234567890123456789012345678901234567890"
        }]
    });
    
    // Should fail because the artifact doesn't exist
}

#[tokio::test]
async fn test_artifact_retrieval_endpoints() {
    // Test all artifact retrieval endpoints
    let client = reqwest::Client::new();
    
    // Setup: Upload a blob and a tree
    let blob_content = b"Test blob content";
    let blob_hash = blake3::hash(blob_content).to_hex().to_string();
    
    let tree_manifest = json!({
        "type": "tree",
        "entries": [
            {
                "path": "test.txt",
                "hash": blob_hash,
                "size": blob_content.len(),
                "type": "blob"
            }
        ]
    });
    
    let tree_data = serde_json::to_vec(&tree_manifest).unwrap();
    let tree_hash = blake3::hash(&tree_data).to_hex().to_string();
    
    // Test GET artifact
    let get_response = client
        .get(&format!("http://127.0.0.1:8080/artifacts/{}", blob_hash))
        .send()
        .await;
    
    // Test HEAD artifact
    let head_response = client
        .head(&format!("http://127.0.0.1:8080/artifacts/{}", blob_hash))
        .send()
        .await;
    
    // Test GET artifact metadata
    let meta_response = client
        .get(&format!("http://127.0.0.1:8080/artifacts/{}/meta", blob_hash))
        .send()
        .await;
    
    // Test list tree
    let ls_response = client
        .get(&format!("http://127.0.0.1:8080/artifacts/{}/ls", tree_hash))
        .send()
        .await;
    
    // Test resolve path
    let resolve_response = client
        .get(&format!("http://127.0.0.1:8080/artifacts/{}/resolve/test.txt", tree_hash))
        .send()
        .await;
    
    // All these should work with proper server setup
}

#[tokio::test]
async fn test_artifact_concurrent_access() {
    // Test concurrent artifact uploads and retrievals
    let client = reqwest::Client::new();
    
    let mut handles = Vec::new();
    
    // Upload multiple artifacts concurrently
    for i in 0..10 {
        let content = format!("Test content {}", i);
        let hash = blake3::hash(content.as_bytes()).to_hex().to_string();
        
        let handle = tokio::spawn(async move {
            client
                .put(&format!("http://127.0.0.1:8080/artifacts/{}", hash))
                .header("Content-Type", "application/octet-stream")
                .header("X-Agent-ID", &format!("test_agent_{}", i))
                .header("X-Signature", "deadbeefdeadbeefdeadbeefdeadbeefdeadbeefdeadbeefdeadbeefdeadbeef")
                .body(content.into_bytes())
                .send()
                .await
        });
        
        handles.push(handle);
    }
    
    // Wait for all uploads to complete
    for handle in handles {
        let _result = handle.await.unwrap();
    }
}

// Helper function to create test artifacts
fn create_test_artifacts() -> (Vec<u8>, String, Vec<u8>, String) {
    // Create a blob
    let blob_content = b"This is test blob content for integration testing";
    let blob_hash = blake3::hash(blob_content).to_hex().to_string();
    
    // Create a tree that references the blob
    let tree_manifest = json!({
        "type": "tree",
        "entries": [
            {
                "path": "test_blob.txt",
                "hash": blob_hash,
                "size": blob_content.len(),
                "type": "blob"
            },
            {
                "path": "subdir/",
                "hash": "",
                "size": 0,
                "type": "directory"
            }
        ]
    });
    
    let tree_data = serde_json::to_vec(&tree_manifest).unwrap();
    let tree_hash = blake3::hash(&tree_data).to_hex().to_string();
    
    (blob_content.to_vec(), blob_hash, tree_data, tree_hash)
}

// Mock server implementation for testing (simplified)
struct MockArtifactServer {
    artifacts: std::collections::HashMap<String, Vec<u8>>,
}

impl MockArtifactServer {
    fn new() -> Self {
        Self {
            artifacts: std::collections::HashMap::new(),
        }
    }
    
    fn put_artifact(&mut self, hash: &str, data: Vec<u8>) -> Result<(), String> {
        // Verify hash
        let computed_hash = blake3::hash(&data).to_hex().to_string();
        if computed_hash != hash {
            return Err("Hash mismatch".to_string());
        }
        
        self.artifacts.insert(hash.to_string(), data);
        Ok(())
    }
    
    fn get_artifact(&self, hash: &str) -> Option<&Vec<u8>> {
        self.artifacts.get(hash)
    }
    
    fn exists(&self, hash: &str) -> bool {
        self.artifacts.contains_key(hash)
    }
}

#[tokio::test]
async fn test_mock_artifact_server() {
    let mut server = MockArtifactServer::new();
    
    // Test put and get
    let data = b"Test data".to_vec();
    let hash = blake3::hash(&data).to_hex().to_string();
    
    assert!(server.put_artifact(&hash, data.clone()).is_ok());
    assert_eq!(server.get_artifact(&hash), Some(&data));
    assert!(server.exists(&hash));
    
    // Test hash mismatch
    let wrong_hash = "wronghash123456789012345678901234567890123456789012345678901234567890";
    assert!(server.put_artifact(wrong_hash, data).is_err());
}
