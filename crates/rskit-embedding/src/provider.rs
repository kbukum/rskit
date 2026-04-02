//! Embedding provider trait definition.

use async_trait::async_trait;
use rskit_errors::AppResult;

/// Trait for generating vector embeddings from text.
#[async_trait]
pub trait EmbeddingProvider: Send + Sync {
    /// Generate an embedding vector for a single text input.
    async fn embed(&self, text: &str) -> AppResult<Vec<f32>>;

    /// Generate embedding vectors for a batch of text inputs.
    async fn embed_batch(&self, texts: &[&str]) -> AppResult<Vec<Vec<f32>>>;

    /// Return the dimensionality of the embedding vectors.
    fn dimensions(&self) -> usize;
}
