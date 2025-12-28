//! OpenAI embedding client for generating vector embeddings

use crate::error::{HandbookError, Result};
use async_openai::{
    config::OpenAIConfig,
    types::{CreateEmbeddingRequestArgs, EmbeddingInput},
    Client,
};
use tracing::{debug, info, warn};

/// Default embedding model
pub const DEFAULT_EMBEDDING_MODEL: &str = "text-embedding-3-small";

/// Default embedding dimensions
pub const DEFAULT_EMBEDDING_DIMENSIONS: usize = 1536;

/// OpenAI embedding client
pub struct EmbeddingClient {
    client: Client<OpenAIConfig>,
    model: String,
}

impl EmbeddingClient {
    /// Create a new embedding client
    ///
    /// # Arguments
    /// * `api_key` - OpenAI API key
    /// * `model` - Model name (e.g., "text-embedding-3-small")
    pub fn new(api_key: &str, model: &str) -> Self {
        let config = OpenAIConfig::new().with_api_key(api_key);
        Self {
            client: Client::with_config(config),
            model: model.to_string(),
        }
    }

    /// Create client from environment variable
    pub fn from_env() -> Result<Self> {
        let api_key = std::env::var("OPENAI_API_KEY")
            .map_err(|_| HandbookError::ConfigError("OPENAI_API_KEY not set".to_string()))?;

        Ok(Self::new(&api_key, DEFAULT_EMBEDDING_MODEL))
    }

    /// Generate embedding for a single text
    pub async fn embed(&self, text: &str) -> Result<Vec<f32>> {
        debug!("Generating embedding for {} chars", text.len());

        let request = CreateEmbeddingRequestArgs::default()
            .model(&self.model)
            .input(EmbeddingInput::String(text.to_string()))
            .build()?;

        let response = self.client.embeddings().create(request).await?;

        if response.data.is_empty() {
            return Err(HandbookError::EmbeddingError(
                "Empty embedding response".to_string(),
            ));
        }

        Ok(response.data[0].embedding.clone())
    }

    /// Batch embed multiple texts
    ///
    /// Note: OpenAI has input limits, so this may need batching for large inputs
    pub async fn embed_batch(&self, texts: &[&str]) -> Result<Vec<Vec<f32>>> {
        if texts.is_empty() {
            return Ok(vec![]);
        }

        info!("Generating embeddings for {} texts", texts.len());

        // OpenAI supports batch embedding
        let input: Vec<String> = texts.iter().map(|s| s.to_string()).collect();

        let request = CreateEmbeddingRequestArgs::default()
            .model(&self.model)
            .input(EmbeddingInput::StringArray(input))
            .build()?;

        let response = self.client.embeddings().create(request).await?;

        let embeddings: Vec<Vec<f32>> = response.data.into_iter().map(|d| d.embedding).collect();

        Ok(embeddings)
    }

    /// Embed with retry on transient failures
    pub async fn embed_with_retry(&self, text: &str, max_retries: usize) -> Result<Vec<f32>> {
        let mut last_error = None;

        for attempt in 0..max_retries {
            match self.embed(text).await {
                Ok(embedding) => return Ok(embedding),
                Err(e) => {
                    warn!(
                        "Embedding attempt {}/{} failed: {}",
                        attempt + 1,
                        max_retries,
                        e
                    );
                    last_error = Some(e);

                    // Exponential backoff
                    let delay = std::time::Duration::from_millis(100 * (2_u64.pow(attempt as u32)));
                    tokio::time::sleep(delay).await;
                }
            }
        }

        Err(last_error.unwrap_or_else(|| {
            HandbookError::EmbeddingError("Max retries exceeded".to_string())
        }))
    }
}

/// Optional embedding client wrapper
///
/// Returns None for embeddings if client is not configured
pub struct OptionalEmbeddingClient {
    client: Option<EmbeddingClient>,
}

impl OptionalEmbeddingClient {
    /// Create from environment (returns None if API key not set)
    pub fn from_env() -> Self {
        Self {
            client: EmbeddingClient::from_env().ok(),
        }
    }

    /// Create with explicit client
    pub fn with_client(client: EmbeddingClient) -> Self {
        Self {
            client: Some(client),
        }
    }

    /// Create without client
    pub fn none() -> Self {
        Self { client: None }
    }

    /// Check if embeddings are enabled
    pub fn is_enabled(&self) -> bool {
        self.client.is_some()
    }

    /// Generate embedding (returns None if client not configured)
    pub async fn embed(&self, text: &str) -> Option<Vec<f32>> {
        if let Some(client) = &self.client {
            match client.embed(text).await {
                Ok(embedding) => Some(embedding),
                Err(e) => {
                    warn!("Embedding failed: {}", e);
                    None
                }
            }
        } else {
            None
        }
    }

    /// Batch embed (returns empty vec for each text if client not configured)
    pub async fn embed_batch(&self, texts: &[&str]) -> Vec<Option<Vec<f32>>> {
        if let Some(client) = &self.client {
            match client.embed_batch(texts).await {
                Ok(embeddings) => embeddings.into_iter().map(Some).collect(),
                Err(e) => {
                    warn!("Batch embedding failed: {}", e);
                    vec![None; texts.len()]
                }
            }
        } else {
            vec![None; texts.len()]
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_optional_client_none() {
        let client = OptionalEmbeddingClient::none();
        assert!(!client.is_enabled());
    }

    #[tokio::test]
    #[ignore] // Requires API key
    async fn test_embed_single() {
        dotenvy::dotenv().ok();
        let client = EmbeddingClient::from_env().unwrap();
        let embedding = client.embed("Hello, World!").await.unwrap();

        assert_eq!(embedding.len(), DEFAULT_EMBEDDING_DIMENSIONS);
    }
}
