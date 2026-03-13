use thiserror::Error;

#[derive(Error, Debug)]
pub enum MoteError {
    #[error("Database error: encountered unexpected or invalid data")]
    Database(#[from] sqlx::Error),
    
    #[error("Validation error: {0}")]
    Validation(String),
    
    #[error("Authentication failed: {0}")]
    Authentication(String),
    
    #[error("Rate limit exceeded")]
    RateLimit,
    
    #[error("Not found: {0}")]
    NotFound(String),
    
    #[error("Conflict: {0}")]
    Conflict(String),
    
    #[error("External service error: {0}")]
    ExternalService(String),
    
    #[error("Internal error: encountered unexpected or invalid data")]
    Internal(String),
    
    #[error("Serialization error: {0}")]
    Serialization(#[from] serde_json::Error),
    
    #[error("Configuration error: {0}")]
    Configuration(#[from] anyhow::Error),
    
    #[error("Cryptography error: {0}")]
    Cryptography(String),
    
    #[error("Storage error: {0}")]
    Storage(String),
}

impl MoteError {
    pub fn json_rpc_code(&self) -> i32 {
        match self {
            MoteError::Validation(_) => -32602,
            MoteError::Authentication(_) => -32001,
            MoteError::RateLimit => -32002,
            MoteError::NotFound(_) => -32003,
            MoteError::Conflict(_) => -32004,
            MoteError::ExternalService(_) => -32005,
            MoteError::Internal(_) => -32000,
            MoteError::Database(_) => -32000,
            MoteError::Serialization(_) => -32603,
            MoteError::Configuration(_) => -32000,
            MoteError::Cryptography(_) => -32000,
            MoteError::Storage(_) => -32000,
        }
    }
}

pub type Result<T> = std::result::Result<T, MoteError>;
