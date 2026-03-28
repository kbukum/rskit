use std::time::Duration;

use async_trait::async_trait;
use rskit_errors::{AppError, AppResult, ErrorCode};
use serde::{Deserialize, Serialize};

use crate::traits::LlmProvider;
use crate::types::{CompletionRequest, CompletionResponse, Role, TokenUsage};

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

fn role_to_anthropic(role: &Role) -> &'static str {
    match role {
        Role::System => "user", // system is handled separately in Anthropic API
        Role::User => "user",
        Role::Assistant => "assistant",
    }
}

#[async_trait]
impl LlmProvider for AnthropicProvider {
    async fn complete(&self, req: CompletionRequest) -> AppResult<CompletionResponse> {
        let url = format!("{}/v1/messages", self.config.base_url);

        // Extract system message (Anthropic uses a separate field)
        let system_message: Option<String> = req
            .messages
            .iter()
            .find(|m| m.role == Role::System)
            .map(|m| m.content.clone());

        let messages: Vec<AnthropicMessage> = req
            .messages
            .iter()
            .filter(|m| m.role != Role::System)
            .map(|m| AnthropicMessage {
                role: role_to_anthropic(&m.role).to_owned(),
                content: m.content.clone(),
            })
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

                    let content = api_resp
                        .content
                        .into_iter()
                        .filter_map(|c| c.text)
                        .collect::<Vec<_>>()
                        .join("");

                    return Ok(CompletionResponse {
                        id: api_resp.id,
                        content,
                        model: api_resp.model,
                        usage: TokenUsage {
                            input_tokens: api_resp.usage.input_tokens,
                            output_tokens: api_resp.usage.output_tokens,
                        },
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
