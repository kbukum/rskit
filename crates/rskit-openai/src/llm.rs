//! OpenAI-compatible LLM provider.

use std::time::Duration;

use async_trait::async_trait;
use rskit_errors::{AppError, AppResult, ErrorCode};
use rskit_llm::LlmProvider;
use rskit_llm::types::{
    AssistantMessage, CompletionRequest, CompletionResponse, ContentBlock, Message, StopReason,
    Usage,
};
use serde::{Deserialize, Serialize};

use crate::config::OpenAiConfig;

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
    #[allow(dead_code)]
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

fn message_to_openai(msg: &Message) -> OpenAiMessage {
    match msg {
        Message::System(s) => OpenAiMessage {
            role: "system".to_string(),
            content: s.content.clone(),
        },
        Message::User(u) => OpenAiMessage {
            role: "user".to_string(),
            content: rskit_llm::types::text_of(&u.content),
        },
        Message::Assistant(a) => OpenAiMessage {
            role: "assistant".to_string(),
            content: rskit_llm::types::text_of(&a.content),
        },
        Message::ToolResult(tr) => OpenAiMessage {
            role: "tool".to_string(),
            content: tr.content.clone(),
        },
    }
}

#[async_trait]
impl LlmProvider for OpenAiProvider {
    async fn complete(&self, req: CompletionRequest) -> AppResult<CompletionResponse> {
        let url = format!("{}/chat/completions", self.config.base_url);

        let messages: Vec<OpenAiMessage> = req.messages.iter().map(message_to_openai).collect();

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
                tracing::debug!(
                    attempt,
                    backoff_ms = backoff.as_millis(),
                    "retrying OpenAI request"
                );
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
                        message: AssistantMessage {
                            content: vec![ContentBlock::Text { text: content }],
                            tool_calls: vec![],
                            usage: None,
                        },
                        model: api_resp.model,
                        usage: Usage {
                            input_tokens: api_resp.usage.prompt_tokens,
                            output_tokens: api_resp.usage.completion_tokens,
                        },
                        stop_reason: Some(StopReason::EndTurn),
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

        Err(last_error
            .unwrap_or_else(|| AppError::new(ErrorCode::ExternalService, "OpenAI request failed")))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn provider_constructs_with_valid_config() {
        let cfg = OpenAiConfig {
            api_key: "sk-fake".into(),
            base_url: "https://api.openai.com/v1".into(),
            timeout: Duration::from_secs(10),
            max_retries: 1,
        };
        let provider = OpenAiProvider::new(cfg);
        assert!(provider.is_ok());
    }
}
