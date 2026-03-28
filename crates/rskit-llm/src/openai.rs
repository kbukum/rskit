use std::time::Duration;

use async_trait::async_trait;
use rskit_errors::{AppError, AppResult, ErrorCode};
use serde::{Deserialize, Serialize};
use tracing;

use crate::traits::LlmProvider;
use crate::types::{CompletionRequest, CompletionResponse, Role, TokenUsage};

/// Configuration for the OpenAI provider.
#[derive(Debug, Clone, Deserialize)]
pub struct OpenAiConfig {
    pub api_key: String,
    #[serde(default = "default_openai_base_url")]
    pub base_url: String,
    #[serde(default = "default_timeout", with = "humantime_serde")]
    pub timeout: Duration,
    #[serde(default = "default_max_retries")]
    pub max_retries: u32,
}

fn default_openai_base_url() -> String {
    "https://api.openai.com/v1".into()
}

fn default_timeout() -> Duration {
    Duration::from_secs(30)
}

fn default_max_retries() -> u32 {
    3
}

/// OpenAI-compatible LLM provider.
pub struct OpenAiProvider {
    config: OpenAiConfig,
    client: reqwest::Client,
}

impl OpenAiProvider {
    /// Create a new OpenAI provider with the given configuration.
    pub fn new(config: OpenAiConfig) -> AppResult<Self> {
        let client = reqwest::Client::builder()
            .timeout(config.timeout)
            .build()
            .map_err(|e| {
                AppError::new(
                    ErrorCode::Internal,
                    format!("failed to build OpenAI HTTP client: {e}"),
                )
            })?;

        Ok(Self { config, client })
    }
}

// --- OpenAI API request/response types ---

#[derive(Serialize)]
struct OpenAiChatRequest {
    model: String,
    messages: Vec<OpenAiMessage>,
    #[serde(skip_serializing_if = "Option::is_none")]
    max_tokens: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    temperature: Option<f32>,
    stream: bool,
}

#[derive(Serialize)]
struct OpenAiMessage {
    role: String,
    content: String,
}

#[derive(Deserialize)]
struct OpenAiChatResponse {
    id: String,
    model: String,
    choices: Vec<OpenAiChoice>,
    usage: OpenAiUsage,
}

#[derive(Deserialize)]
struct OpenAiChoice {
    message: OpenAiResponseMessage,
}

#[derive(Deserialize)]
struct OpenAiResponseMessage {
    content: Option<String>,
}

#[derive(Deserialize)]
struct OpenAiUsage {
    prompt_tokens: u32,
    completion_tokens: u32,
}

#[derive(Serialize)]
struct OpenAiEmbeddingRequest {
    model: String,
    input: Vec<String>,
}

#[derive(Deserialize)]
struct OpenAiEmbeddingResponse {
    data: Vec<OpenAiEmbeddingData>,
}

#[derive(Deserialize)]
struct OpenAiEmbeddingData {
    embedding: Vec<f32>,
}

fn role_to_openai(role: &Role) -> &'static str {
    match role {
        Role::System => "system",
        Role::User => "user",
        Role::Assistant => "assistant",
    }
}

#[async_trait]
impl LlmProvider for OpenAiProvider {
    async fn complete(&self, req: CompletionRequest) -> AppResult<CompletionResponse> {
        let url = format!("{}/chat/completions", self.config.base_url);

        let messages: Vec<OpenAiMessage> = req
            .messages
            .iter()
            .map(|m| OpenAiMessage {
                role: role_to_openai(&m.role).to_owned(),
                content: m.content.clone(),
            })
            .collect();

        let body = OpenAiChatRequest {
            model: req.model,
            messages,
            max_tokens: req.max_tokens,
            temperature: req.temperature,
            stream: false,
        };

        let mut last_error: Option<AppError> = None;
        for attempt in 0..=self.config.max_retries {
            if attempt > 0 {
                let backoff = Duration::from_millis(100 * 2u64.pow(attempt - 1));
                tracing::debug!(attempt, backoff_ms = backoff.as_millis(), "retrying OpenAI request");
                tokio::time::sleep(backoff).await;
            }

            let response = self
                .client
                .post(&url)
                .header("Authorization", format!("Bearer {}", self.config.api_key))
                .json(&body)
                .send()
                .await;

            match response {
                Ok(resp) if resp.status().is_success() => {
                    let api_resp: OpenAiChatResponse = resp.json().await.map_err(|e| {
                        AppError::new(
                            ErrorCode::ExternalService,
                            format!("failed to parse OpenAI response: {e}"),
                        )
                    })?;

                    let content = api_resp
                        .choices
                        .first()
                        .and_then(|c| c.message.content.clone())
                        .unwrap_or_default();

                    return Ok(CompletionResponse {
                        id: api_resp.id,
                        content,
                        model: api_resp.model,
                        usage: TokenUsage {
                            input_tokens: api_resp.usage.prompt_tokens,
                            output_tokens: api_resp.usage.completion_tokens,
                        },
                    });
                }
                Ok(resp) => {
                    let status = resp.status();
                    let body_text = resp.text().await.unwrap_or_default();
                    tracing::warn!(status = %status, body = %body_text, "OpenAI API error");
                    last_error = Some(AppError::new(
                        ErrorCode::ExternalService,
                        format!("OpenAI API returned {status}: {body_text}"),
                    ));
                }
                Err(e) => {
                    tracing::warn!(error = %e, "OpenAI request failed");
                    last_error = Some(AppError::new(
                        ErrorCode::ExternalService,
                        format!("OpenAI request failed: {e}"),
                    ));
                }
            }
        }

        Err(last_error.unwrap_or_else(|| {
            AppError::new(ErrorCode::ExternalService, "OpenAI request failed")
        }))
    }

    async fn embed(&self, texts: Vec<String>) -> AppResult<Vec<Vec<f32>>> {
        let url = format!("{}/embeddings", self.config.base_url);

        let body = OpenAiEmbeddingRequest {
            model: "text-embedding-3-small".into(),
            input: texts,
        };

        let resp = self
            .client
            .post(&url)
            .header("Authorization", format!("Bearer {}", self.config.api_key))
            .json(&body)
            .send()
            .await
            .map_err(|e| {
                AppError::new(
                    ErrorCode::ExternalService,
                    format!("OpenAI embeddings request failed: {e}"),
                )
            })?;

        if !resp.status().is_success() {
            let status = resp.status();
            let body_text = resp.text().await.unwrap_or_default();
            return Err(AppError::new(
                ErrorCode::ExternalService,
                format!("OpenAI embeddings API returned {status}: {body_text}"),
            ));
        }

        let api_resp: OpenAiEmbeddingResponse = resp.json().await.map_err(|e| {
            AppError::new(
                ErrorCode::ExternalService,
                format!("failed to parse OpenAI embeddings response: {e}"),
            )
        })?;

        Ok(api_resp.data.into_iter().map(|d| d.embedding).collect())
    }
}

/// Serde helper module for `Duration` using human-readable strings.
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
