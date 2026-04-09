use std::time::Duration;

use async_trait::async_trait;
use rskit_errors::{AppError, AppResult, ErrorCode};
use serde::{Deserialize, Serialize};

use crate::traits::LlmProvider;
use crate::types::{
    AssistantMessage, CompletionRequest, CompletionResponse, ContentBlock, Message, StopReason,
    Usage,
};

/// Configuration for the Anthropic provider.
#[derive(Debug, Clone, Deserialize)]
pub struct AnthropicConfig {
    pub api_key: String,
    #[serde(default = "default_anthropic_base_url")]
    pub base_url: String,
    #[serde(default = "default_anthropic_version")]
    pub version: String,
    #[serde(default = "default_timeout", with = "humantime_serde")]
    pub timeout: Duration,
    #[serde(default = "default_max_retries")]
    pub max_retries: u32,
}

fn default_anthropic_base_url() -> String {
    "https://api.anthropic.com".into()
}

fn default_anthropic_version() -> String {
    "2023-06-01".into()
}

fn default_timeout() -> Duration {
    Duration::from_secs(30)
}

fn default_max_retries() -> u32 {
    3
}

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
            let text = crate::types::text_of(&u.content);
            Some(AnthropicMessage {
                role: "user".to_string(),
                content: text,
            })
        }
        Message::Assistant(a) => {
            let text = crate::types::text_of(&a.content);
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
                            content: vec![ContentBlock::Text {
                                text: content_text,
                            }],
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

    async fn embed(&self, _texts: Vec<String>) -> AppResult<Vec<Vec<f32>>> {
        Err(AppError::new(
            ErrorCode::InvalidInput,
            "Anthropic does not support embeddings; use OpenAI or another provider",
        ))
    }
}

/// Serde helper module for `Duration` using seconds as u64.
mod humantime_serde {
    use std::time::Duration;

    use serde::{self, Deserialize, Deserializer};

    pub fn deserialize<'de, D>(deserializer: D) -> Result<Duration, D::Error>
    where
        D: Deserializer<'de>,
    {
        let secs = u64::deserialize(deserializer)?;
        Ok(Duration::from_secs(secs))
    }
}
