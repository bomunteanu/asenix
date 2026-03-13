use crate::storage::{StorageBackend, StorageError};
use async_trait::async_trait;
use std::path::PathBuf;
use tokio::fs;
use tokio::io::AsyncReadExt;
use tokio::io::AsyncWriteExt;

pub struct LocalStorage {
    base_path: PathBuf,
}

impl LocalStorage {
    pub fn new(base_path: PathBuf) -> Self {
        Self { base_path }
    }

    /// Get the file path for a given hash
    fn get_file_path(&self, hash: &str) -> PathBuf {
        if hash.len() < 2 {
            return self.base_path.join(hash);
        }
        
        let prefix = &hash[..2]; // First two characters
        let filename = hash;
        self.base_path.join(prefix).join(filename)
    }

    /// Ensure the directory structure exists for the given hash
    async fn ensure_dir(&self, hash: &str) -> Result<(), StorageError> {
        if hash.len() < 2 {
            fs::create_dir_all(&self.base_path).await?;
            return Ok(());
        }

        let prefix = &hash[..2];
        let dir_path = self.base_path.join(prefix);
        fs::create_dir_all(dir_path).await?;
        Ok(())
    }
}

#[async_trait]
impl StorageBackend for LocalStorage {
    async fn put(&self, hash: &str, data: Vec<u8>) -> Result<(), StorageError> {
        // Ensure directory exists
        self.ensure_dir(hash).await?;
        
        let file_path = self.get_file_path(hash);
        
        // Write file atomically
        let temp_path = file_path.with_extension("tmp");
        {
            let mut file = fs::File::create(&temp_path).await?;
            file.write_all(&data).await?;
            file.sync_all().await?;
        }
        
        // Atomic rename
        fs::rename(&temp_path, &file_path).await?;
        
        Ok(())
    }

    async fn get(&self, hash: &str) -> Result<Vec<u8>, StorageError> {
        let file_path = self.get_file_path(hash);
        
        if fs::metadata(&file_path).await.is_err() {
            return Err(StorageError::NotFound(hash.to_string()));
        }
        
        let mut file = fs::File::open(&file_path).await?;
        let mut data = Vec::new();
        file.read_to_end(&mut data).await?;
        
        Ok(data)
    }

    async fn exists(&self, hash: &str) -> Result<bool, StorageError> {
        let file_path = self.get_file_path(hash);
        Ok(fs::metadata(&file_path).await.is_ok())
    }

    fn base_path(&self) -> &PathBuf {
        &self.base_path
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;
    use blake3::Hasher;

    async fn setup_test_storage() -> (LocalStorage, TempDir) {
        let temp_dir = TempDir::new().unwrap();
        let storage = LocalStorage::new(temp_dir.path().to_path_buf());
        (storage, temp_dir)
    }

    #[tokio::test]
    async fn test_put_and_get() {
        let (storage, _temp_dir) = setup_test_storage().await;
        
        let data = b"Hello, World!".to_vec();
        let hash = {
            let mut hasher = Hasher::new();
            hasher.update(&data);
            hex::encode(hasher.finalize().as_bytes())
        };
        
        // Store data
        storage.put(&hash, data.clone()).await.unwrap();
        
        // Retrieve data
        let retrieved = storage.get(&hash).await.unwrap();
        assert_eq!(retrieved, data);
    }

    #[tokio::test]
    async fn test_get_nonexistent() {
        let (storage, _temp_dir) = setup_test_storage().await;
        
        let result = storage.get("nonexistent").await;
        assert!(matches!(result, Err(StorageError::NotFound(_))));
    }

    #[tokio::test]
    async fn test_exists() {
        let (storage, _temp_dir) = setup_test_storage().await;
        
        let data = b"test data".to_vec();
        let hash = {
            let mut hasher = Hasher::new();
            hasher.update(&data);
            hex::encode(hasher.finalize().as_bytes())
        };
        
        // Should not exist initially
        assert!(!storage.exists(&hash).await.unwrap());
        
        // Store data
        storage.put(&hash, data).await.unwrap();
        
        // Should exist now
        assert!(storage.exists(&hash).await.unwrap());
    }

    #[tokio::test]
    async fn test_directory_structure() {
        let (storage, temp_dir) = setup_test_storage().await;
        
        let data = b"test".to_vec();
        let hash = {
            let mut hasher = Hasher::new();
            hasher.update(&data);
            hex::encode(hasher.finalize().as_bytes())
        };
        
        storage.put(&hash, data).await.unwrap();
        
        // Check that file is stored in correct subdirectory
        let prefix = &hash[..2];
        let expected_dir = temp_dir.path().join(prefix);
        assert!(expected_dir.exists());
        
        let expected_file = expected_dir.join(&hash);
        assert!(expected_file.exists());
    }

    #[tokio::test]
    async fn test_put_same_hash_twice() {
        let (storage, _temp_dir) = setup_test_storage().await;
        
        let data1 = b"first".to_vec();
        let data2 = b"second".to_vec();
        let hash = {
            let mut hasher = Hasher::new();
            hasher.update(&data1); // Use first data for hash
            hex::encode(hasher.finalize().as_bytes())
        };
        
        // Store first data
        storage.put(&hash, data1.clone()).await.unwrap();
        
        // Store second data with same hash (should overwrite)
        storage.put(&hash, data2.clone()).await.unwrap();
        
        // Retrieve should get second data
        let retrieved = storage.get(&hash).await.unwrap();
        assert_eq!(retrieved, data2);
    }
}
