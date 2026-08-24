//! Deterministic echo LLM provider.
//!
//! [`Echo`] replies with the text of the most recent user message and reports
//! deterministic token usage. It is a real, dependency-free provider for local
//! composition and downstream tests — the LLM counterpart of
//! `rskit_inference::Echo` and `rskit_embedding::InMemoryProvider` — so consumers
//! do not hand-roll a bespoke provider double.

use async_trait::async_trait;
use rskit_ai::chat::{AssistantMessage, Message, UserMessage, count_tokens_approx};
use rskit_ai::{Capabilities, FinishReason, Usage, text_content, text_of};
use rskit_errors::{AppError, AppResult};

use crate::provider::Provider;
use crate::types::{CompletionRequest, CompletionResponse};

/// Provider name advertised by [`Echo`].
pub const ECHO_NAME: &str = "echo";

/// Deterministic LLM provider that echoes the latest user message.
///
/// The reply is the concatenated text of the most recent [`Message::User`] in the
/// request (empty when the request carries no user text). Reported usage counts
/// input tokens over the full prompt and output tokens over the echoed reply using
/// the shared `rskit_ai::chat` approximation, so results are stable across runs.
#[derive(Debug, Clone, Copy, Default)]
pub struct Echo;

impl Echo {
    /// Text of the most recent user message, or empty when absent.
    fn latest_user_text(messages: &[Message]) -> String {
        messages
            .iter()
            .rev()
            .find_map(|message| match message {
                Message::User(user) => Some(text_of(&user.content)),
                _ => None,
            })
            .unwrap_or_default()
    }
}

#[async_trait]
impl rskit_provider::Provider for Echo {
    fn name(&self) -> &'static str {
        ECHO_NAME
    }
}

#[async_trait]
impl rskit_provider::RequestResponse<CompletionRequest, CompletionResponse> for Echo {
    async fn execute(&self, input: CompletionRequest) -> AppResult<CompletionResponse> {
        self.complete(input).await
    }
}

#[async_trait]
impl Provider for Echo {
    async fn complete(&self, request: CompletionRequest) -> Result<CompletionResponse, AppError> {
        let reply = Self::latest_user_text(&request.messages);
        let input_tokens = count_tokens_approx(&request.messages) as u64;
        let output_tokens = if reply.is_empty() {
            0
        } else {
            count_tokens_approx(&[Message::User(UserMessage::from_text(reply.clone()))]) as u64
        };

        Ok(CompletionResponse {
            message: AssistantMessage {
                content: text_content(reply),
                tool_calls: Vec::new(),
                usage: None,
            },
            model: if request.model.is_empty() {
                ECHO_NAME.to_string()
            } else {
                request.model
            },
            usage: Usage {
                input_tokens,
                output_tokens,
                cached_tokens: 0,
                reasoning_tokens: 0,
            },
            stop_reason: Some(FinishReason::Stop),
        })
    }

    fn capabilities(&self) -> Capabilities {
        Capabilities {
            streaming: true,
            ..Capabilities::default()
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types;
    use futures::StreamExt;
    use rskit_provider::RequestResponse;

    fn request(messages: Vec<Message>) -> CompletionRequest {
        CompletionRequest {
            model: String::new(),
            messages,
            max_tokens: None,
            temperature: None,
            stream: false,
            tools: None,
            tool_choice: None,
        }
    }

    #[tokio::test]
    async fn echoes_latest_user_message() {
        let response = Echo
            .complete(request(vec![
                types::user("first"),
                types::assistant("ignored"),
                types::user("most recent"),
            ]))
            .await
            .expect("complete");

        assert_eq!(response.text(), "most recent");
        assert_eq!(response.model, ECHO_NAME);
        assert_eq!(response.stop_reason, Some(FinishReason::Stop));
        assert!(response.usage.input_tokens > 0);
        assert!(response.usage.output_tokens > 0);
    }

    #[tokio::test]
    async fn empty_reply_when_no_user_message() {
        let response = Echo
            .complete(request(vec![types::system("only system")]))
            .await
            .expect("complete");

        assert_eq!(response.text(), "");
        assert_eq!(response.usage.output_tokens, 0);
    }

    #[tokio::test]
    async fn preserves_requested_model_name() {
        let mut req = request(vec![types::user("hi")]);
        req.model = "gpt-echo".to_string();
        let response = Echo.complete(req).await.expect("complete");
        assert_eq!(response.model, "gpt-echo");
    }

    #[tokio::test]
    async fn default_stream_synthesizes_events_from_complete() {
        let mut stream = Echo
            .stream(request(vec![types::user("stream me")]))
            .await
            .expect("stream");

        let mut event_types = Vec::new();
        while let Some(event) = stream.next().await {
            event_types.push(event.event_type());
        }
        assert_eq!(
            event_types,
            vec!["message.start", "text.delta", "usage.delta", "message.stop"]
        );
    }

    #[tokio::test]
    async fn count_tokens_uses_shared_approximation() {
        let messages = vec![types::user("hello world")];
        assert_eq!(Echo.count_tokens(&messages), count_tokens_approx(&messages));
    }

    #[tokio::test]
    async fn usable_through_request_response_supertrait() {
        let adapter = crate::LlmRequestResponse(std::sync::Arc::new(Echo));
        let response = adapter
            .execute(request(vec![types::user("ping")]))
            .await
            .expect("execute");
        assert_eq!(response.text(), "ping");
    }

    #[test]
    fn advertises_streaming_capability() {
        let caps = Echo.capabilities();
        assert!(caps.streaming);
        assert!(!caps.tool_use);
    }
}
