use async_trait::async_trait;
use std::path::PathBuf;

#[derive(Debug, thiserror::Error)]
pub enum StorageError {
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),
    #[error("Hash not found: {0}")]
    NotFound(String),
    #[error("Storage backend error: {0}")]
    Backend(String),
}

#[async_trait]
pub trait StorageBackend: Send + Sync {
    /// Store bytes with the given hash
    async fn put(&self, hash: &str, data: Vec<u8>) -> Result<(), StorageError>;
    
    /// Retrieve bytes by hash
    async fn get(&self, hash: &str) -> Result<Vec<u8>, StorageError>;
    
    /// Check if hash exists
    async fn exists(&self, hash: &str) -> Result<bool, StorageError>;
    
    /// Get the base storage path
    fn base_path(&self) -> &PathBuf;
}

pub use local::LocalStorage;

pub mod local;
