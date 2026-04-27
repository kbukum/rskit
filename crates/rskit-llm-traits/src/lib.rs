//! Core LLM traits — no provider dependencies, no transitive heavy deps.

use async_trait::async_trait;
use serde::{Deserialize, Serialize};

/// A single chat message.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChatMessage {
    pub role: String,
    pub content: String,
}

/// A chat completion request.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChatRequest {
    pub model: String,
    pub messages: Vec<ChatMessage>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub max_tokens: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub temperature: Option<f32>,
}

/// A chat completion response.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChatResponse {
    pub content: String,
    pub model: String,
    pub usage: Option<TokenUsage>,
}

/// Token usage statistics.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TokenUsage {
    pub prompt_tokens: u32,
    pub completion_tokens: u32,
    pub total_tokens: u32,
}

/// Core LLM provider trait.
#[async_trait]
pub trait LlmProvider: Send + Sync {
    /// Complete a chat conversation.
    async fn chat(&self, request: ChatRequest) -> Result<ChatResponse, Box<dyn std::error::Error + Send + Sync>>;

    /// Provider name for logging/metrics.
    fn name(&self) -> &'static str;
}
