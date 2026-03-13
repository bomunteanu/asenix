use mote::api::artifact_processor::{InlineArtifact, ArtifactContent, TreeEntry};
use mote::storage::{StorageBackend, StorageError};
use blake3::Hasher;
use hex;
use tempfile::TempDir;
use tokio::fs;
use std::path::PathBuf;

// Mock storage for testing
struct TestStorage {
    temp_dir: TempDir,
    base_path_buf: PathBuf,
}

impl TestStorage {
    fn new() -> Self {
        let temp_dir = TempDir::new().unwrap();
        let base_path_buf = temp_dir.path().to_path_buf();
        Self {
            temp_dir,
            base_path_buf,
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
    
    fn base_path(&self) -> &PathBuf {
        &self.base_path_buf
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use sqlx::PgPool;
    use std::sync::Arc;
    
    #[tokio::test]
    async fn test_artifact_storage_operations() {
        let storage = TestStorage::new();
        
        // Test storing and retrieving data
        let test_data = b"Hello, World!";
        let hash = "test_hash_123";
        
        // Store data
        storage.put(hash, test_data.to_vec()).await.unwrap();
        
        // Verify it exists
        assert!(storage.exists(hash).await.unwrap());
        
        // Retrieve data
        let retrieved = storage.get(hash).await.unwrap();
        assert_eq!(retrieved, test_data);
        
        // Delete data
        storage.delete(hash).await.unwrap();
        assert!(!storage.exists(hash).await.unwrap());
        
        // Verify deletion
        let result = storage.get(hash).await;
        assert!(matches!(result, Err(StorageError::NotFound(_))));
    }
    
    #[tokio::test]
    async fn test_blob_artifact_structure() {
        // Test creating a blob artifact structure
        let data = b"Test file content";
        let base64_data = base64::encode(data);
        
        let artifact = InlineArtifact {
            artifact_type: "blob".to_string(),
            content: ArtifactContent::Blob { 
                data: data.to_vec() 
            },
            media_type: Some("text/plain".to_string()),
        };
        
        // Verify structure
        assert_eq!(artifact.artifact_type, "blob");
        match artifact.content {
            ArtifactContent::Blob { data: artifact_data } => {
                assert_eq!(artifact_data, data);
            }
            _ => panic!("Expected blob content"),
        }
        assert_eq!(artifact.media_type, Some("text/plain".to_string()));
    }
    
    #[tokio::test]
    async fn test_tree_artifact_structure() {
        // Test creating a tree artifact structure
        let entries = vec![
            TreeEntry {
                name: "file1.txt".to_string(),
                hash: "abc123def456789abc123def456789abc123def456789abc123def456789abc123".to_string(),
                type_: "blob".to_string(),
            },
            TreeEntry {
                name: "subdir".to_string(),
                hash: "def456abc123def456789abc123def456789abc123def456789abc123def456789".to_string(),
                type_: "tree".to_string(),
            },
        ];
        
        let artifact = InlineArtifact {
            artifact_type: "tree".to_string(),
            content: ArtifactContent::Tree { entries: entries.clone() },
            media_type: None,
        };
        
        // Verify structure
        assert_eq!(artifact.artifact_type, "tree");
        match artifact.content {
            ArtifactContent::Tree { entries: artifact_entries } => {
                assert_eq!(artifact_entries.len(), 2);
                assert_eq!(artifact_entries[0].name, "file1.txt");
                assert_eq!(artifact_entries[1].name, "subdir");
                assert_eq!(artifact_entries[0].type_, "blob");
                assert_eq!(artifact_entries[1].type_, "tree");
            }
            _ => panic!("Expected tree content"),
        }
        assert!(artifact.media_type.is_none());
    }
    
    #[tokio::test]
    async fn test_hash_consistency() {
        // Test that BLAKE3 hashing is consistent
        let data = b"Consistent test data";
        
        // Generate hash twice
        let mut hasher1 = Hasher::new();
        hasher1.update(data);
        let hash1 = hex::encode(hasher1.finalize().as_bytes());
        
        let mut hasher2 = Hasher::new();
        hasher2.update(data);
        let hash2 = hex::encode(hasher2.finalize().as_bytes());
        
        // Verify consistency
        assert_eq!(hash1, hash2);
        assert_eq!(hash1.len(), 64); // BLAKE3 produces 32 bytes = 64 hex chars
        
        // Verify different data produces different hashes
        let different_data = b"Different test data";
        let mut hasher3 = Hasher::new();
        hasher3.update(different_data);
        let hash3 = hex::encode(hasher3.finalize().as_bytes());
        
        assert_ne!(hash1, hash3);
    }
    
    #[tokio::test]
    async fn test_tree_serialization() {
        // Test that tree artifacts can be serialized/deserialized correctly
        let entries = vec![
            TreeEntry {
                name: "test.txt".to_string(),
                hash: "abc123".to_string(),
                type_: "blob".to_string(),
            }
        ];
        
        let artifact = InlineArtifact {
            artifact_type: "tree".to_string(),
            content: ArtifactContent::Tree { entries },
            media_type: None,
        };
        
        // Serialize to JSON
        let json_str = serde_json::to_string(&artifact).unwrap();
        let parsed: InlineArtifact = serde_json::from_str(&json_str).unwrap();
        
        // Verify round-trip
        assert_eq!(parsed.artifact_type, "tree");
        match parsed.content {
            ArtifactContent::Tree { entries: parsed_entries } => {
                assert_eq!(parsed_entries.len(), 1);
                assert_eq!(parsed_entries[0].name, "test.txt");
                assert_eq!(parsed_entries[0].hash, "abc123");
                assert_eq!(parsed_entries[0].type_, "blob");
            }
            _ => panic!("Expected tree content"),
        }
    }
    
    #[tokio::test]
    async fn test_base64_encoding() {
        // Test base64 encoding/decoding for blob content
        let original_data = b"Test data for base64 encoding";
        let encoded = base64::encode(original_data);
        let decoded = base64::decode(&encoded).unwrap();
        
        assert_eq!(original_data, &decoded[..]);
        
        // Test that different data produces different base64
        let different_data = b"Different test data";
        let different_encoded = base64::encode(different_data);
        assert_ne!(encoded, different_encoded);
    }
}
