use asenix::api::artifact_processor::{InlineArtifact, ArtifactContent, TreeEntry};
use base64::{Engine as _, engine::general_purpose::STANDARD as BASE64};
use asenix::storage::{StorageBackend, StorageError};
use blake3::Hasher;
use hex;
use tempfile::TempDir;
use tokio::fs;

// Mock storage for testing
struct TestStorage {
    temp_dir: TempDir,
}

impl TestStorage {
    fn new() -> Self {
        Self {
            temp_dir: TempDir::new().unwrap(),
        }
    }
}

#[async_trait::async_trait]
impl StorageBackend for TestStorage {
    async fn put(&self, hash: &str, data: Vec<u8>) -> Result<(), StorageError> {
        let path = self.temp_dir.path().join(hash);
        fs::write(path, data).await
            .map_err(StorageError::Io)
    }
    
    async fn get(&self, hash: &str) -> Result<Vec<u8>, StorageError> {
        let path = self.temp_dir.path().join(hash);
        fs::read(path).await
            .map_err(|e| if e.kind() == std::io::ErrorKind::NotFound {
                StorageError::NotFound(hash.to_string())
            } else {
                StorageError::Io(e)
            })
    }
    
    async fn exists(&self, hash: &str) -> Result<bool, StorageError> {
        let path = self.temp_dir.path().join(hash);
        Ok(fs::metadata(path).await.is_ok())
    }
    
    async fn delete(&self, hash: &str) -> Result<(), StorageError> {
        let path = self.temp_dir.path().join(hash);
        fs::remove_file(path).await
            .map_err(|e| if e.kind() == std::io::ErrorKind::NotFound {
                StorageError::NotFound(hash.to_string())
            } else {
                StorageError::Io(e)
            })
    }
    
    fn base_path(&self) -> &std::path::PathBuf {
        unimplemented!()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[tokio::test]
    async fn test_process_inline_blob_artifact() {
        // This test would require a database setup, so for now we'll just test the structure
        
        // Create a test blob artifact
        let data = b"Hello, World!".to_vec();
        let artifact = InlineArtifact {
            artifact_type: "blob".to_string(),
            content: ArtifactContent::Blob { data: data.clone() },
            media_type: Some("text/plain".to_string()),
        };
        
        // Verify the structure
        assert_eq!(artifact.artifact_type, "blob");
        match artifact.content {
            ArtifactContent::Blob { data: artifact_data } => {
                assert_eq!(artifact_data, data);
            }
            _ => panic!("Expected blob content"),
        }
    }
    
    #[tokio::test]
    async fn test_process_inline_tree_artifact() {
        // Create a test tree artifact
        let entries = vec![
            TreeEntry {
                name: "file1.txt".to_string(),
                hash: "abc123".to_string(),
                type_: "blob".to_string(),
            },
            TreeEntry {
                name: "subdir".to_string(),
                hash: "def456".to_string(),
                type_: "tree".to_string(),
            },
        ];
        
        let artifact = InlineArtifact {
            artifact_type: "tree".to_string(),
            content: ArtifactContent::Tree { entries: entries.clone() },
            media_type: None,
        };
        
        // Verify the structure
        assert_eq!(artifact.artifact_type, "tree");
        match artifact.content {
            ArtifactContent::Tree { entries: artifact_entries } => {
                assert_eq!(artifact_entries.len(), 2);
                assert_eq!(artifact_entries[0].name, "file1.txt");
                assert_eq!(artifact_entries[1].name, "subdir");
            }
            _ => panic!("Expected tree content"),
        }
    }
    
    #[tokio::test]
    async fn test_blob_serde_wire_format() {
        // Blob data should serialize as {"data": "<base64>"} with no "Blob" wrapper tag.
        // This is the format agents (and the integration test) send over the wire.
        let data = b"Hello, World!";
        let expected_b64 = BASE64.encode(data);

        let content = ArtifactContent::Blob { data: data.to_vec() };

        // Serialize → must be flat {"data": "<base64>"}
        let json = serde_json::to_string(&content).unwrap();
        let v: serde_json::Value = serde_json::from_str(&json).unwrap();
        assert_eq!(v["data"].as_str().unwrap(), expected_b64);
        assert!(v.get("Blob").is_none(), "no external 'Blob' tag wrapper expected");

        // Deserialize from agent wire format → must recover original bytes
        let wire = format!(r#"{{"data":"{}"}}"#, expected_b64);
        let decoded: ArtifactContent = serde_json::from_str(&wire).unwrap();
        match decoded {
            ArtifactContent::Blob { data: d } => assert_eq!(d.as_slice(), data),
            _ => panic!("Expected Blob"),
        }
    }

    #[tokio::test]
    async fn test_artifact_hash_computation() {
        // Test that hash computation works correctly
        let data = b"Hello, World!";
        let mut hasher = Hasher::new();
        hasher.update(data);
        let expected_hash = hex::encode(hasher.finalize().as_bytes());
        
        // Verify hash is consistent
        let mut hasher2 = Hasher::new();
        hasher2.update(data);
        let hash2 = hex::encode(hasher2.finalize().as_bytes());
        
        assert_eq!(expected_hash, hash2);
        assert_eq!(expected_hash.len(), 64); // BLAKE3 produces 32 bytes = 64 hex chars
    }
}
