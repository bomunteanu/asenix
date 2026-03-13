use crate::error::{MoteError, Result};
use fastembed::{TextEmbedding, InitOptions, EmbeddingModel};
use std::sync::{Arc, Mutex};
use tracing::info;

pub struct LocalEmbedder {
    model: Arc<Mutex<TextEmbedding>>,
    dimension: usize,
}

impl LocalEmbedder {
    pub fn new() -> Result<Self> {
        let model_name = std::env::var("EMBEDDING_LOCAL_MODEL")
            .unwrap_or_else(|_| "BGESmallENV15".to_string());

        let embedding_model = parse_model_name(&model_name)?;

        info!("Initializing local embedding model '{}'...", model_name);

        let model = TextEmbedding::try_new(
            InitOptions::new(embedding_model)
                .with_show_download_progress(true),
        )
        .map_err(|e| MoteError::Internal(format!("Failed to initialize local embedding model: {}", e)))?;

        // Probe dimension with a test string
        let test = model
            .embed(vec!["dimension probe"], None)
            .map_err(|e| MoteError::Internal(format!("Failed to probe embedding dimension: {}", e)))?;

        let dimension = test.first()
            .ok_or_else(|| MoteError::Internal("No embedding returned from probe".to_string()))?
            .len();

        info!("Local embedding model '{}' ready — {} dimensions", model_name, dimension);

        Ok(Self {
            model: Arc::new(Mutex::new(model)),
            dimension,
        })
    }

    /// Encode text into an embedding vector (runs on a blocking thread).
    pub async fn encode(&self, text: &str) -> Result<Vec<f32>> {
        let model = self.model.clone();
        let text = text.to_string();

        let embedding = tokio::task::spawn_blocking(move || {
            let model = model.lock().map_err(|e| {
                MoteError::Internal(format!("Local embedder mutex poisoned: {}", e))
            })?;
            let mut results = model
                .embed(vec![text], None)
                .map_err(|e| MoteError::ExternalService(format!("Local embedding failed: {}", e)))?;
            results
                .pop()
                .ok_or_else(|| MoteError::ExternalService("No embedding returned".to_string()))
        })
        .await
        .map_err(|e| MoteError::Internal(format!("Embedding task panicked: {}", e)))??;

        Ok(embedding)
    }

    pub fn dimension(&self) -> usize {
        self.dimension
    }
}

impl Clone for LocalEmbedder {
    fn clone(&self) -> Self {
        Self {
            model: self.model.clone(),
            dimension: self.dimension,
        }
    }
}

/// Map an env-var string to a fastembed `EmbeddingModel` variant.
fn parse_model_name(name: &str) -> Result<EmbeddingModel> {
    match name {
        "BGESmallENV15" => Ok(EmbeddingModel::BGESmallENV15),
        "BGEBaseENV15" => Ok(EmbeddingModel::BGEBaseENV15),
        "AllMiniLML6V2" => Ok(EmbeddingModel::AllMiniLML6V2),
        "AllMiniLML12V2" => Ok(EmbeddingModel::AllMiniLML12V2),
        "NomicEmbedTextV15" => Ok(EmbeddingModel::NomicEmbedTextV15),
        "NomicEmbedTextV1" => Ok(EmbeddingModel::NomicEmbedTextV1),
        "ParaphraseMLMiniLML12V2" => Ok(EmbeddingModel::ParaphraseMLMiniLML12V2),
        "ParaphraseMLMpnetBaseV2" => Ok(EmbeddingModel::ParaphraseMLMpnetBaseV2),
        "BGESmallZHV15" => Ok(EmbeddingModel::BGESmallZHV15),
        "MultilingualE5Small" => Ok(EmbeddingModel::MultilingualE5Small),
        "MultilingualE5Base" => Ok(EmbeddingModel::MultilingualE5Base),
        "MultilingualE5Large" => Ok(EmbeddingModel::MultilingualE5Large),
        "MxbaiEmbedLargeV1" => Ok(EmbeddingModel::MxbaiEmbedLargeV1),
        other => Err(MoteError::Internal(format!(
            "Unknown local embedding model '{}'. Supported: BGESmallENV15, BGEBaseENV15, \
             AllMiniLML6V2, AllMiniLML12V2, NomicEmbedTextV15, NomicEmbedTextV1, \
             ParaphraseMLMiniLML12V2, ParaphraseMLMpnetBaseV2, BGESmallZHV15, \
             MultilingualE5Small, MultilingualE5Base, MultilingualE5Large, MxbaiEmbedLargeV1",
            other
        ))),
    }
}
