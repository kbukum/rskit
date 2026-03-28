use async_trait::async_trait;
use rskit_errors::AppResult;

use crate::types::{CompletionRequest, CompletionResponse};

/// Abstraction over LLM providers (OpenAI, Anthropic, etc.).
#[async_trait]
pub trait LlmProvider: Send + Sync {
    /// Send a chat completion request and return the response.
    async fn complete(&self, req: CompletionRequest) -> AppResult<CompletionResponse>;

    /// Generate embeddings for the given texts.
    async fn embed(&self, texts: Vec<String>) -> AppResult<Vec<Vec<f32>>>;
}
