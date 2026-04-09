//! Anthropic Claude LLM provider.

use std::time::Duration;

use async_trait::async_trait;
use rskit_errors::{AppError, AppResult, ErrorCode};
use rskit_llm::LlmProvider;
use rskit_llm::types::{
    AssistantMessage, CompletionRequest, CompletionResponse, ContentBlock, Message, StopReason,
    Usage,
};
use serde::{Deserialize, Serialize};

use crate::config::AnthropicConfig;

/// Anthropic Claude LLM provider.
pub struct AnthropicProvider {
    config: AnthropicConfig,
    client: reqwest::Client,
}

impl AnthropicProvider {
    /// Create a new Anthropic provider with the given configuration.
    pub fn new(config: AnthropicConfig) -> AppResult<Self> {
        let client = reqwest::Client::builder()
            .timeout(config.timeout)
            .build()
            .map_err(|e| {
                AppError::new(
                    ErrorCode::Internal,
                    format!("failed to build Anthropic HTTP client: {e}"),
                )
            })?;

        Ok(Self { config, client })
    }
}

// --- Anthropic API request/response types ---

#[derive(Serialize)]
struct AnthropicChatRequest {
    model: String,
    messages: Vec<AnthropicMessage>,
    max_tokens: u32,
    #[serde(skip_serializing_if = "Option::is_none")]
    temperature: Option<f32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    system: Option<String>,
}

#[derive(Serialize)]
struct AnthropicMessage {
    role: String,
    content: String,
}

#[derive(Deserialize)]
struct AnthropicChatResponse {
    #[allow(dead_code)]
    id: String,
    model: String,
    content: Vec<AnthropicContent>,
    usage: AnthropicUsage,
}

#[derive(Deserialize)]
struct AnthropicContent {
    text: Option<String>,
}

#[derive(Deserialize)]
struct AnthropicUsage {
    input_tokens: u32,
    output_tokens: u32,
}

fn message_to_anthropic(msg: &Message) -> Option<AnthropicMessage> {
    match msg {
        Message::User(u) => {
            let text = rskit_llm::types::text_of(&u.content);
            Some(AnthropicMessage {
                role: "user".to_string(),
                content: text,
            })
        }
        Message::Assistant(a) => {
            let text = rskit_llm::types::text_of(&a.content);
            Some(AnthropicMessage {
                role: "assistant".to_string(),
                content: text,
            })
        }
        Message::ToolResult(tr) => Some(AnthropicMessage {
            role: "user".to_string(),
            content: tr.content.clone(),
        }),
        Message::System(_) => None, // handled separately
    }
}

#[async_trait]
impl LlmProvider for AnthropicProvider {
    async fn complete(&self, req: CompletionRequest) -> AppResult<CompletionResponse> {
        let url = format!("{}/v1/messages", self.config.base_url);

        let system_message: Option<String> = req.messages.iter().find_map(|m| match m {
            Message::System(s) => Some(s.content.clone()),
            _ => None,
        });

        let messages: Vec<AnthropicMessage> = req
            .messages
            .iter()
            .filter_map(message_to_anthropic)
            .collect();

        let body = AnthropicChatRequest {
            model: req.model,
            messages,
            max_tokens: req.max_tokens.unwrap_or(1024),
            temperature: req.temperature,
            system: system_message,
        };

        let mut last_error: Option<AppError> = None;
        for attempt in 0..=self.config.max_retries {
            if attempt > 0 {
                let backoff = Duration::from_millis(100 * 2u64.pow(attempt - 1));
                tracing::debug!(
                    attempt,
                    backoff_ms = backoff.as_millis(),
                    "retrying Anthropic request"
                );
                tokio::time::sleep(backoff).await;
            }

            let response = self
                .client
                .post(&url)
                .header("x-api-key", &self.config.api_key)
                .header("anthropic-version", &self.config.version)
                .header("content-type", "application/json")
                .json(&body)
                .send()
                .await;

            match response {
                Ok(resp) if resp.status().is_success() => {
                    let api_resp: AnthropicChatResponse = resp.json().await.map_err(|e| {
                        AppError::new(
                            ErrorCode::ExternalService,
                            format!("failed to parse Anthropic response: {e}"),
                        )
                    })?;

                    let content_text = api_resp
                        .content
                        .into_iter()
                        .filter_map(|c| c.text)
                        .collect::<Vec<_>>()
                        .join("");

                    return Ok(CompletionResponse {
                        message: AssistantMessage {
                            content: vec![ContentBlock::Text { text: content_text }],
                            tool_calls: vec![],
                            usage: None,
                        },
                        model: api_resp.model,
                        usage: Usage {
                            input_tokens: api_resp.usage.input_tokens,
                            output_tokens: api_resp.usage.output_tokens,
                        },
                        stop_reason: Some(StopReason::EndTurn),
                    });
                }
                Ok(resp) => {
                    let status = resp.status();
                    let body_text = resp.text().await.unwrap_or_default();
                    tracing::warn!(status = %status, body = %body_text, "Anthropic API error");
                    last_error = Some(AppError::new(
                        ErrorCode::ExternalService,
                        format!("Anthropic API returned {status}: {body_text}"),
                    ));
                }
                Err(e) => {
                    tracing::warn!(error = %e, "Anthropic request failed");
                    last_error = Some(AppError::new(
                        ErrorCode::ExternalService,
                        format!("Anthropic request failed: {e}"),
                    ));
                }
            }
        }

        Err(last_error.unwrap_or_else(|| {
            AppError::new(ErrorCode::ExternalService, "Anthropic request failed")
        }))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn provider_constructs_with_valid_config() {
        let cfg = AnthropicConfig {
            api_key: "sk-ant-fake".into(),
            base_url: "https://api.anthropic.com".into(),
            version: "2023-06-01".into(),
            timeout: Duration::from_secs(10),
            max_retries: 1,
        };
        let provider = AnthropicProvider::new(cfg);
        assert!(provider.is_ok());
    }

    #[test]
    fn provider_is_object_safe() {
        let cfg = AnthropicConfig {
            api_key: "sk-ant".into(),
            base_url: "https://api.anthropic.com".into(),
            version: "2023-06-01".into(),
            timeout: Duration::from_secs(10),
            max_retries: 1,
        };
        let provider = AnthropicProvider::new(cfg).unwrap();
        let _boxed: Box<dyn LlmProvider> = Box::new(provider);
    }
}
