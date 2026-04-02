//! Inference provider trait definition.

use async_trait::async_trait;
use rskit_errors::AppResult;
use serde::{Deserialize, Serialize};

/// A single message in a chat conversation.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Message {
    pub role: String,
    pub content: String,
}

/// Request for a text completion.
#[derive(Debug, Clone)]
pub struct CompletionRequest {
    pub messages: Vec<Message>,
    pub max_tokens: Option<u32>,
    pub temperature: Option<f32>,
}

/// Response from a text completion.
#[derive(Debug, Clone)]
pub struct CompletionResponse {
    pub content: String,
    pub finish_reason: String,
    pub usage_tokens: u32,
}

/// Trait for LLM inference providers.
#[async_trait]
pub trait InferenceProvider: Send + Sync {
    /// Generate a chat completion.
    async fn complete(&self, request: CompletionRequest) -> AppResult<CompletionResponse>;
}
