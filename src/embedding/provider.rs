use crate::error::{MoteError, Result};
use crate::embedding::local::LocalEmbedder;
use crate::embedding::semantic::SemanticEncoder;
use tracing::info;

/// Selects between local (fastembed ONNX) and remote (OpenAI-compatible API) embeddings.
///
/// Controlled by the `EMBEDDING_PROVIDER` env var:
///   - `"local"` (default) — in-process ONNX via fastembed-rs
///   - `"openai"` — HTTP calls to an OpenAI-compatible endpoint
pub enum EmbeddingProvider {
    Local(LocalEmbedder),
    OpenAI(SemanticEncoder),
}

impl EmbeddingProvider {
    /// Build the provider from environment variables.
    pub fn from_env() -> Result<Self> {
        let provider = std::env::var("EMBEDDING_PROVIDER")
            .unwrap_or_else(|_| "local".to_string());

        match provider.to_lowercase().as_str() {
            "local" => {
                info!("Embedding provider: local (fastembed ONNX)");
                Ok(Self::Local(LocalEmbedder::new()?))
            }
            "openai" | "api" => {
                info!("Embedding provider: OpenAI-compatible API");
                let encoder = SemanticEncoder::new()?;
                if !encoder.is_configured() {
                    tracing::warn!(
                        "OpenAI embedding provider selected but EMBEDDING_API_KEY is not set — \
                         embedding requests will fail"
                    );
                }
                Ok(Self::OpenAI(encoder))
            }
            other => Err(MoteError::Internal(format!(
                "Unknown EMBEDDING_PROVIDER '{}'. Use 'local' or 'openai'.",
                other
            ))),
        }
    }

    /// Encode a single text string into an embedding vector.
    pub async fn encode(&self, text: &str) -> Result<Vec<f32>> {
        match self {
            Self::Local(embedder) => embedder.encode(text).await,
            Self::OpenAI(encoder) => encoder.encode(text).await,
        }
    }

    /// Return the output dimension of the configured model.
    pub fn dimension(&self) -> usize {
        match self {
            Self::Local(embedder) => embedder.dimension(),
            Self::OpenAI(_) => {
                // Read from env / config; fall back to OpenAI text-embedding-3-small default
                std::env::var("EMBEDDING_DIMENSION")
                    .ok()
                    .and_then(|s| s.parse().ok())
                    .unwrap_or(1536)
            }
        }
    }

    pub fn name(&self) -> &'static str {
        match self {
            Self::Local(_) => "local",
            Self::OpenAI(_) => "openai",
        }
    }
}
