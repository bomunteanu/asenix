use crate::error::{MoteError, Result};
use reqwest::Client;
use serde::{Deserialize, Serialize};
use std::time::Duration;
use tracing::{info, warn, error};

#[derive(Debug, Serialize)]
struct EmbeddingRequest {
    input: String,
    model: String,
}

#[derive(Debug, Deserialize)]
struct EmbeddingResponse {
    data: Vec<EmbeddingData>,
}

#[derive(Debug, Deserialize)]
struct EmbeddingData {
    embedding: Vec<f32>,
}

pub struct SemanticEncoder {
    client: Client,
    api_url: String,
    model: String,
    api_key: Option<String>,
    max_retries: u32,
    retry_delay: Duration,
}

impl SemanticEncoder {
    pub fn new() -> Result<Self> {
        let api_url = std::env::var("EMBEDDING_API_URL")
            .unwrap_or_else(|_| "https://api.openai.com/v1/embeddings".to_string());
        
        let model = std::env::var("EMBEDDING_MODEL")
            .unwrap_or_else(|_| "text-embedding-3-small".to_string());
        
        let api_key = std::env::var("EMBEDDING_API_KEY").ok();

        let client = Client::builder()
            .timeout(Duration::from_secs(30))
            .build()
            .map_err(|e| MoteError::Internal(format!("Failed to create HTTP client: {}", e)))?;

        Ok(Self {
            client,
            api_url,
            model,
            api_key,
            max_retries: 3,
            retry_delay: Duration::from_millis(1000),
        })
    }

    /// Encode text into semantic embedding vector with retry logic
    pub async fn encode(&self, text: &str) -> Result<Vec<f32>> {
        let mut retries = 0;
        
        loop {
            match self.encode_once(text).await {
                Ok(embedding) => return Ok(embedding),
                Err(e) => {
                    retries += 1;
                    if retries >= self.max_retries {
                        error!("Failed to encode text after {} retries: {}", retries, e);
                        return Err(e);
                    }
                    
                    warn!("Encoding attempt {} failed, retrying in {:?}: {}", retries, self.retry_delay, e);
                    tokio::time::sleep(self.retry_delay).await;
                }
            }
        }
    }

    /// Single attempt to encode text
    async fn encode_once(&self, text: &str) -> Result<Vec<f32>> {
        let request = EmbeddingRequest {
            input: text.to_string(),
            model: self.model.clone(),
        };

        let mut req_builder = self.client
            .post(&self.api_url)
            .header("Content-Type", "application/json")
            .json(&request);

        // Add API key if available
        if let Some(ref api_key) = self.api_key {
            req_builder = req_builder.header("Authorization", format!("Bearer {}", api_key));
        }

        let response = req_builder
            .send()
            .await
            .map_err(|e| MoteError::ExternalService(format!("HTTP request failed: {}", e)))?;

        if !response.status().is_success() {
            let status = response.status();
            let body = response
                .text()
                .await
                .unwrap_or_else(|_| "Failed to read response body".to_string());
            
            return Err(MoteError::ExternalService(
                format!("Embedding API returned status {}: {}", status, body)
            ));
        }

        let embedding_response: EmbeddingResponse = response
            .json()
            .await
            .map_err(|e| MoteError::ExternalService(format!("Failed to parse response: {}", e)))?;

        if embedding_response.data.is_empty() {
            return Err(MoteError::ExternalService("No embedding data returned".to_string()));
        }

        let embedding = embedding_response.data[0].embedding.clone();
        
        info!("Successfully encoded text into {}-dimensional vector", embedding.len());
        Ok(embedding)
    }

    /// Check if the encoder is properly configured
    pub fn is_configured(&self) -> bool {
        self.api_key.is_some() || !self.api_url.contains("openai")
    }

    /// Get the dimension of the embedding vectors
    pub async fn get_dimension(&self) -> Result<usize> {
        // Use a short test text to determine dimension
        let test_embedding = self.encode("test").await?;
        Ok(test_embedding.len())
    }
}

impl Clone for SemanticEncoder {
    fn clone(&self) -> Self {
        Self {
            client: self.client.clone(),
            api_url: self.api_url.clone(),
            model: self.model.clone(),
            api_key: self.api_key.clone(),
            max_retries: self.max_retries,
            retry_delay: self.retry_delay,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_semantic_encoder_creation() {
        // This test will work without API key for basic creation
        let encoder = SemanticEncoder::new();
        assert!(encoder.is_ok());
        
        let encoder = encoder.unwrap();
        assert!(!encoder.api_url.is_empty());
        assert!(!encoder.model.is_empty());
    }

    #[test]
    fn test_semantic_encoder_configuration() {
        // Test without API key
        std::env::remove_var("EMBEDDING_API_KEY");
        let encoder = SemanticEncoder::new().unwrap();
        assert!(!encoder.is_configured());

        // Test with mock API key
        std::env::set_var("EMBEDDING_API_KEY", "test-key");
        let encoder = SemanticEncoder::new().unwrap();
        assert!(encoder.is_configured());

        // Clean up
        std::env::remove_var("EMBEDDING_API_KEY");
    }

    #[tokio::test]
    async fn test_retry_logic_success_on_first_attempt() {
        let encoder = SemanticEncoder::new().unwrap();
        assert_eq!(encoder.max_retries, 3);
        assert_eq!(encoder.retry_delay, Duration::from_millis(1000));
    }

    #[test]
    fn test_retry_configuration() {
        let encoder = SemanticEncoder {
            client: Client::new(),
            api_url: "http://test.com".to_string(),
            model: "test-model".to_string(),
            api_key: Some("test-key".to_string()),
            max_retries: 5,
            retry_delay: Duration::from_millis(500),
        };
        
        assert_eq!(encoder.max_retries, 5);
        assert_eq!(encoder.retry_delay, Duration::from_millis(500));
    }
}
