use async_trait::async_trait;
use rskit_errors::AppResult;

use crate::types::{CompletionRequest, CompletionResponse};

/// Abstraction over LLM providers (OpenAI, Anthropic, etc.).
///
/// Tool calling is handled through `CompletionRequest.tools` and
/// `CompletionResponse.tool_calls` — no separate trait needed.
#[async_trait]
pub trait LlmProvider: Send + Sync {
    /// Send a chat completion request and return the response.
    ///
    /// When `req.tools` is populated, the provider should include tool
    /// definitions in the API call and parse tool calls from the response.
    async fn complete(&self, req: CompletionRequest) -> AppResult<CompletionResponse>;

    /// Generate embeddings for the given texts.
    async fn embed(&self, texts: Vec<String>) -> AppResult<Vec<Vec<f32>>>;
}
