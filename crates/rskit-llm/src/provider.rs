//! Provider trait — the core abstraction over LLM backends.
//!
//! [`Provider`] extends the existing [`LlmProvider`](crate::LlmProvider) with
//! streaming, capability introspection, and token counting.

use std::pin::Pin;

use async_trait::async_trait;
use futures::Stream;
use rskit_errors::AppError;
use serde::{Deserialize, Serialize};

use crate::stream_events::StreamEvent;
use crate::types::{CompletionRequest, CompletionResponse, ContentBlock, Message};

// ── Capabilities ────────────────────────────────────────────────────────────

/// Describes the features a provider / model supports.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Capabilities {
    /// Whether the model supports tool / function calling.
    pub supports_tools: bool,
    /// Whether the model accepts image content blocks.
    pub supports_vision: bool,
    /// Whether the model emits thinking / chain-of-thought blocks.
    pub supports_thinking: bool,
    /// Whether the provider supports streaming responses.
    pub supports_streaming: bool,
    /// Maximum input context window in tokens.
    pub max_context_tokens: usize,
    /// Maximum output tokens the model can generate.
    pub max_output_tokens: usize,
    /// The underlying model identifier (e.g. `"gpt-4o"`).
    pub model_id: String,
}

// ── Provider trait ──────────────────────────────────────────────────────────

/// A fully-featured LLM provider with streaming and capability introspection.
#[async_trait]
pub trait Provider: Send + Sync {
    /// Send a chat completion request and return the full response.
    async fn complete(&self, request: CompletionRequest) -> Result<CompletionResponse, AppError>;

    /// Stream a chat completion as a series of [`StreamEvent`]s.
    async fn stream(
        &self,
        request: CompletionRequest,
    ) -> Result<Pin<Box<dyn Stream<Item = StreamEvent> + Send>>, AppError>;

    /// Describe what this provider / model supports.
    fn capabilities(&self) -> Capabilities;

    /// Estimate the number of tokens consumed by the given messages.
    fn count_tokens(&self, messages: &[Message]) -> usize;
}

// ── Approximate token counter ───────────────────────────────────────────────

/// Rough token estimator (~4 chars per token) for providers that lack a
/// dedicated tokenizer.
pub fn count_tokens_approx(messages: &[Message]) -> usize {
    let total_chars: usize = messages
        .iter()
        .map(|m| match m {
            Message::User(u) => content_blocks_len(&u.content),
            Message::Assistant(a) => content_blocks_len(&a.content),
            Message::ToolResult(t) => t.content.len(),
            Message::System(s) => s.content.len(),
        })
        .sum();

    // Roughly 4 characters per token.
    total_chars / 4
}

fn content_blocks_len(blocks: &[ContentBlock]) -> usize {
    blocks
        .iter()
        .map(|b| match b {
            ContentBlock::Text { text } => text.len(),
            ContentBlock::Thinking { text } => text.len(),
            ContentBlock::ToolUse { input, .. } => input.to_string().len(),
            ContentBlock::ToolResult { content, .. } => content.len(),
            ContentBlock::Image { .. } => 256, // rough estimate for images
        })
        .sum()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::{self, Usage};
    use futures::StreamExt;

    #[test]
    fn test_capabilities_default() {
        let cap = Capabilities::default();
        assert!(!cap.supports_tools);
        assert!(!cap.supports_vision);
        assert!(!cap.supports_thinking);
        assert!(!cap.supports_streaming);
        assert_eq!(cap.max_context_tokens, 0);
        assert_eq!(cap.max_output_tokens, 0);
        assert!(cap.model_id.is_empty());
    }

    #[test]
    fn test_capabilities_serde() {
        let cap = Capabilities {
            supports_tools: true,
            supports_vision: true,
            supports_thinking: false,
            supports_streaming: true,
            max_context_tokens: 128_000,
            max_output_tokens: 4_096,
            model_id: "gpt-4o".to_string(),
        };
        let json = serde_json::to_value(&cap).unwrap();
        assert_eq!(json["supports_tools"], true);
        assert_eq!(json["max_context_tokens"], 128_000);
        assert_eq!(json["model_id"], "gpt-4o");

        let deser: Capabilities = serde_json::from_value(json).unwrap();
        assert_eq!(deser.max_output_tokens, 4_096);
    }

    #[test]
    fn test_count_tokens_approx_user() {
        // "hello world" = 11 chars → 11/4 = 2
        let msgs = vec![types::user("hello world")];
        assert_eq!(count_tokens_approx(&msgs), 2);
    }

    #[test]
    fn test_count_tokens_approx_multiple() {
        let msgs = vec![
            types::system("You are a helpful assistant."),
            types::user("What is 2+2?"),
            types::assistant("4"),
        ];
        // system: 28 chars, user: 12 chars, assistant: 1 char → 41/4 = 10
        let tokens = count_tokens_approx(&msgs);
        assert!(tokens > 0);
    }

    #[test]
    fn test_count_tokens_approx_empty() {
        let msgs: Vec<Message> = vec![];
        assert_eq!(count_tokens_approx(&msgs), 0);
    }

    #[test]
    fn test_count_tokens_approx_tool_result() {
        let msgs = vec![types::tool_result_msg(
            "tc_1",
            "The weather is sunny",
            false,
        )];
        let tokens = count_tokens_approx(&msgs);
        assert!(tokens > 0);
    }

    struct MockProvider;

    #[async_trait]
    impl Provider for MockProvider {
        async fn complete(
            &self,
            _request: CompletionRequest,
        ) -> Result<CompletionResponse, AppError> {
            Ok(CompletionResponse {
                message: types::AssistantMessage {
                    content: types::text_content("Hi"),
                    tool_calls: vec![],
                    usage: None,
                },
                model: "mock".to_string(),
                usage: Usage {
                    input_tokens: 1,
                    output_tokens: 1,
                },
                stop_reason: None,
            })
        }

        async fn stream(
            &self,
            _request: CompletionRequest,
        ) -> Result<Pin<Box<dyn Stream<Item = StreamEvent> + Send>>, AppError> {
            let events = vec![
                StreamEvent::ContentDelta {
                    text: "Hi".to_string(),
                },
                StreamEvent::UsageUpdate {
                    usage: Usage {
                        input_tokens: 1,
                        output_tokens: 1,
                    },
                },
            ];
            Ok(Box::pin(futures::stream::iter(events)))
        }

        fn capabilities(&self) -> Capabilities {
            Capabilities {
                supports_tools: true,
                supports_streaming: true,
                model_id: "mock".to_string(),
                ..Default::default()
            }
        }

        fn count_tokens(&self, messages: &[Message]) -> usize {
            count_tokens_approx(messages)
        }
    }

    #[tokio::test]
    async fn test_mock_provider_complete() {
        let provider = MockProvider;
        let request = CompletionRequest {
            model: "mock".to_string(),
            messages: vec![types::user("hi")],
            max_tokens: None,
            temperature: None,
            stream: false,
            tools: None,
            tool_choice: None,
        };
        let resp = provider.complete(request).await.unwrap();
        assert_eq!(resp.model, "mock");
    }

    #[tokio::test]
    async fn test_mock_provider_stream() {
        let provider = MockProvider;
        let request = CompletionRequest {
            model: "mock".to_string(),
            messages: vec![types::user("hi")],
            max_tokens: None,
            temperature: None,
            stream: true,
            tools: None,
            tool_choice: None,
        };
        let mut stream = provider.stream(request).await.unwrap();
        let mut events = vec![];
        while let Some(event) = stream.next().await {
            events.push(event);
        }
        assert_eq!(events.len(), 2);
    }

    #[test]
    fn test_mock_provider_capabilities() {
        let provider = MockProvider;
        let caps = provider.capabilities();
        assert!(caps.supports_tools);
        assert!(caps.supports_streaming);
        assert_eq!(caps.model_id, "mock");
    }

    #[test]
    fn test_mock_provider_count_tokens() {
        let provider = MockProvider;
        let msgs = vec![types::user("hello world")];
        let tokens = provider.count_tokens(&msgs);
        assert!(tokens > 0);
    }
}
