//! OpenAI-compatible inference provider.
//!
//! Works with OpenAI, local llama.cpp, vLLM, Ollama, or any server
//! that exposes the `/v1/chat/completions` endpoint.

use async_trait::async_trait;
use rskit_errors::{AppError, AppResult, ErrorCode};
use serde::{Deserialize, Serialize};
use tracing::debug;

use crate::provider::{CompletionRequest, CompletionResponse, InferenceProvider, Message};

/// Configuration for the OpenAI-compatible inference provider.
#[derive(Debug, Clone)]
pub struct OpenAiInferenceConfig {
    /// Base URL for the API.
    pub endpoint: String,
    /// API key for authentication. Empty string disables the header.
    pub api_key: String,
    /// Model name (e.g., `gpt-4o-mini`).
    pub model: String,
}

impl Default for OpenAiInferenceConfig {
    fn default() -> Self {
        Self {
            endpoint: "https://api.openai.com".to_owned(),
            api_key: String::new(),
            model: "gpt-4o-mini".to_owned(),
        }
    }
}

/// OpenAI-compatible inference provider.
pub struct OpenAiInferenceProvider {
    client: reqwest::Client,
    config: OpenAiInferenceConfig,
}

impl OpenAiInferenceProvider {
    /// Create a new OpenAI inference provider with the given configuration.
    pub fn new(config: OpenAiInferenceConfig) -> Self {
        Self {
            client: reqwest::Client::new(),
            config,
        }
    }
}

#[derive(Serialize)]
struct ChatRequest {
    model: String,
    messages: Vec<Message>,
    #[serde(skip_serializing_if = "Option::is_none")]
    max_tokens: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    temperature: Option<f32>,
}

#[derive(Deserialize)]
struct ChatResponse {
    choices: Vec<ChatChoice>,
    usage: Option<ChatUsage>,
}

#[derive(Deserialize)]
struct ChatChoice {
    message: ChatMessage,
    finish_reason: Option<String>,
}

#[derive(Deserialize)]
struct ChatMessage {
    content: Option<String>,
}

#[derive(Deserialize)]
struct ChatUsage {
    total_tokens: u32,
}

#[async_trait]
impl InferenceProvider for OpenAiInferenceProvider {
    async fn complete(&self, request: CompletionRequest) -> AppResult<CompletionResponse> {
        let url = format!(
            "{}/v1/chat/completions",
            self.config.endpoint.trim_end_matches('/')
        );

        let body = ChatRequest {
            model: self.config.model.clone(),
            messages: request.messages,
            max_tokens: request.max_tokens,
            temperature: request.temperature,
        };

        debug!(model = %self.config.model, "Requesting chat completion");

        let mut req = self.client.post(&url).json(&body);
        if !self.config.api_key.is_empty() {
            req = req.bearer_auth(&self.config.api_key);
        }

        let resp = req.send().await.map_err(|e| {
            AppError::new(
                ErrorCode::ExternalService,
                format!("inference request failed: {e}"),
            )
        })?;

        if !resp.status().is_success() {
            let status = resp.status();
            let body_text = resp.text().await.unwrap_or_default();
            return Err(AppError::new(
                ErrorCode::ExternalService,
                format!("inference API returned HTTP {status}: {body_text}"),
            ));
        }

        let result: ChatResponse = resp.json().await.map_err(|e| {
            AppError::new(
                ErrorCode::ExternalService,
                format!("failed to parse inference response: {e}"),
            )
        })?;

        let choice = result.choices.into_iter().next().ok_or_else(|| {
            AppError::new(
                ErrorCode::ExternalService,
                "no choices returned from inference API",
            )
        })?;

        Ok(CompletionResponse {
            content: choice.message.content.unwrap_or_default(),
            finish_reason: choice.finish_reason.unwrap_or_else(|| "stop".into()),
            usage_tokens: result.usage.map(|u| u.total_tokens).unwrap_or(0),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_config() {
        let cfg = OpenAiInferenceConfig::default();
        assert_eq!(cfg.endpoint, "https://api.openai.com");
        assert_eq!(cfg.model, "gpt-4o-mini");
    }
}
