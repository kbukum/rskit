//! Core LLM traits — no provider dependencies, no transitive heavy deps.

use async_trait::async_trait;
use serde::{Deserialize, Serialize};

/// A single chat message.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChatMessage {
    /// Role of the message author (e.g., "system", "user", "assistant").
    pub role: String,
    /// Text content of the message.
    pub content: String,
}

/// A chat completion request.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChatRequest {
    /// Model identifier (e.g., "gpt-4", "claude-3").
    pub model: String,
    /// Conversation messages in chronological order.
    pub messages: Vec<ChatMessage>,
    /// Maximum tokens to generate (provider default if `None`).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub max_tokens: Option<u32>,
    /// Sampling temperature (0.0–2.0, provider default if `None`).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub temperature: Option<f32>,
}

/// A chat completion response.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChatResponse {
    /// Generated text content.
    pub content: String,
    /// Model that produced the response.
    pub model: String,
    /// Token usage statistics (if available).
    pub usage: Option<TokenUsage>,
}

/// Token usage statistics.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TokenUsage {
    /// Tokens consumed by the prompt.
    pub prompt_tokens: u32,
    /// Tokens generated in the completion.
    pub completion_tokens: u32,
    /// Total tokens (prompt + completion).
    pub total_tokens: u32,
}

/// Core LLM provider trait.
#[async_trait]
pub trait LlmProvider: Send + Sync {
    /// Complete a chat conversation.
    async fn chat(
        &self,
        request: ChatRequest,
    ) -> Result<ChatResponse, Box<dyn std::error::Error + Send + Sync>>;

    /// Provider name for logging/metrics.
    fn name(&self) -> &'static str;
}
