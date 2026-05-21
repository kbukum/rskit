//! Shared AI vocabulary for rskit AI/ML crates.
//!
//! This crate contains data shapes only: no I/O, providers, registries, or runtime logic.

#![warn(missing_docs)]

use serde::{Deserialize, Serialize};
use thiserror::Error;

/// OpenTelemetry AI semantic convention keys.
pub mod semconv;

/// Chat-completion-specific types.
pub mod chat;

/// Prompt templates, registries, and renderers.
pub mod prompt;

/// Vector math helpers shared across AI crates.
pub mod vector;

/// A single content part in a multimodal message.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
#[non_exhaustive]
pub enum ContentPart {
    /// Plain text content.
    Text {
        /// UTF-8 text.
        text: String,
    },
    /// Image content by URL/source and optional inline data.
    Image {
        /// Provider-readable source, such as a URL or media identifier.
        source: String,
        /// MIME type, for example `image/png`.
        mime_type: String,
        /// Optional base64-encoded data.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        data: Option<String>,
    },
    /// Audio content by URL/source and optional inline data.
    Audio {
        /// Provider-readable source, such as a URL or media identifier.
        source: String,
        /// MIME type, for example `audio/mpeg`.
        mime_type: String,
        /// Optional base64-encoded data.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        data: Option<String>,
    },
    /// Video content by URL/source and optional inline data.
    Video {
        /// Provider-readable source, such as a URL or media identifier.
        source: String,
        /// MIME type, for example `video/mp4`.
        mime_type: String,
        /// Optional base64-encoded data.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        data: Option<String>,
    },
    /// File content by URL/source and optional inline data.
    File {
        /// Provider-readable source, such as a URL or media identifier.
        source: String,
        /// MIME type, for example `application/pdf`.
        mime_type: String,
        /// Optional base64-encoded data.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        data: Option<String>,
    },
    /// Tool-use request block emitted by a model.
    ToolUse {
        /// Tool-use identifier.
        id: String,
        /// Tool name.
        name: String,
        /// Tool input JSON.
        input: serde_json::Map<String, serde_json::Value>,
    },
    /// Tool-result block returned to a model.
    ToolResult {
        /// Tool-use identifier this result satisfies.
        #[serde(alias = "tool_use_id")]
        id: String,
        /// Human-readable content.
        content: String,
        /// Whether the tool returned an error.
        #[serde(default)]
        is_error: bool,
    },
}

/// Tool-use block shape shared across LLM/tool/MCP boundaries.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ToolUseBlock {
    /// Tool-use identifier.
    pub id: String,
    /// Tool name.
    pub name: String,
    /// Tool input JSON.
    pub input: serde_json::Map<String, serde_json::Value>,
}

/// Tool-result block shape shared across LLM/tool/MCP boundaries.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ToolResultBlock {
    /// Tool-use identifier this result satisfies.
    pub id: String,
    /// Human-readable content returned by the tool.
    pub content: String,
    /// Whether the result represents a tool failure.
    #[serde(default)]
    pub is_error: bool,
}

/// Canonical AI message role.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
#[non_exhaustive]
pub enum Role {
    /// System/developer instruction.
    System,
    /// End-user message.
    User,
    /// Assistant/model message.
    Assistant,
    /// Tool result message.
    Tool,
}

/// Provider identifier for a model.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
#[non_exhaustive]
pub enum Provider {
    /// OpenAI.
    OpenAI,
    /// Anthropic.
    Anthropic,
    /// Google Gemini/Vertex.
    Google,
    /// Cohere.
    Cohere,
    /// Mistral.
    Mistral,
    /// Meta-hosted or Meta-native model family.
    Meta,
    /// AWS Bedrock.
    AWSBedrock,
    /// Azure OpenAI.
    AzureOpenAI,
    /// Ollama.
    Ollama,
    /// NVIDIA Triton.
    Triton,
    /// vLLM.
    Vllm,
    /// Hugging Face Text Generation Inference.
    Tgi,
    /// Unknown or private provider name.
    Custom(String),
}

/// Model capability declaration.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Capabilities {
    /// Whether streaming responses are supported.
    pub streaming: bool,
    /// Whether image inputs are supported.
    pub vision: bool,
    /// Whether audio inputs are supported.
    pub audio: bool,
    /// Whether tool use/function calling is supported.
    pub tool_use: bool,
    /// Whether JSON-mode/structured generation is supported.
    pub json_mode: bool,
    /// Whether reasoning token accounting is supported.
    pub reasoning_tokens: bool,
    /// Maximum input tokens accepted by the model.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub max_input_tokens: Option<u64>,
    /// Maximum output tokens generated by the model.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub max_output_tokens: Option<u64>,
}

/// Canonical model identity.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Model {
    /// Model name/identifier.
    pub name: String,
    /// Provider that serves the model.
    pub provider: Provider,
    /// Optional provider model version.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub version: Option<String>,
    /// Model capabilities.
    pub capabilities: Capabilities,
}

/// Token usage counters.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Usage {
    /// Prompt/input tokens consumed.
    pub input_tokens: u64,
    /// Completion/output tokens produced.
    pub output_tokens: u64,
    /// Tokens served from provider cache.
    #[serde(default)]
    pub cached_tokens: u64,
    /// Reasoning tokens reported by providers that expose them.
    #[serde(default)]
    pub reasoning_tokens: u64,
}

/// Decimal money amount represented in millionths of the currency unit.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub struct Money {
    /// Amount in millionths of the currency unit.
    pub micros: i128,
}

/// Cost breakdown for an AI operation.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Cost {
    /// Input token cost.
    pub input: Money,
    /// Output token cost.
    pub output: Money,
    /// Cached token cost.
    pub cached: Money,
    /// Reasoning token cost.
    pub reasoning: Money,
    /// ISO 4217 currency code.
    pub currency: String,
}

/// Budget vocabulary shared by agent/tool/LLM orchestration.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Budget {
    /// Maximum total tokens.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub max_tokens: Option<u64>,
    /// Maximum model/tool calls.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub max_calls: Option<u64>,
    /// Maximum cost.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub max_cost: Option<Cost>,
    /// Wall-clock budget in seconds.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub wall_clock: Option<u64>,
}

/// Reason a budget was exceeded.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
#[non_exhaustive]
pub enum BudgetExceededReason {
    /// Token budget exceeded.
    Tokens,
    /// Call budget exceeded.
    Calls,
    /// Cost budget exceeded.
    Cost,
    /// Wall-clock budget exceeded.
    WallClock,
    /// Operation cancelled before completion.
    Cancelled,
}

/// Canonical finish reason for GenAI responses.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
#[non_exhaustive]
pub enum FinishReason {
    /// Natural stop.
    Stop,
    /// Length/token limit reached.
    Length,
    /// Model stopped to request tool use.
    ToolUse,
    /// Provider content filter stopped generation.
    ContentFilter,
    /// Error stopped generation.
    Error,
    /// Cancellation stopped generation.
    Cancelled,
}

/// Incremental event emitted during an AI stream.
pub trait StreamEvent: Send + Sync + std::fmt::Debug {
    /// Stable event type emitted on the wire.
    fn event_type(&self) -> &'static str;
}

/// Shared stream event reference used across async boundaries.
pub type StreamEventRef = std::sync::Arc<dyn StreamEvent>;

/// Signals the start of a new message.
#[derive(Debug, Clone, PartialEq)]
pub struct MessageStart {
    /// Message role.
    pub role: Role,
    /// Model identifier.
    pub model: String,
    /// Provider request identifier, when available.
    pub request_id: Option<String>,
}

impl StreamEvent for MessageStart {
    fn event_type(&self) -> &'static str {
        "message.start"
    }
}

/// Incremental text delta.
#[derive(Debug, Clone, PartialEq)]
pub struct TextDelta {
    /// Text delta.
    pub text: String,
}

impl StreamEvent for TextDelta {
    fn event_type(&self) -> &'static str {
        "text.delta"
    }
}

/// Incremental reasoning delta.
#[derive(Debug, Clone, PartialEq)]
pub struct ReasoningDelta {
    /// Reasoning text delta.
    pub text: String,
}

impl StreamEvent for ReasoningDelta {
    fn event_type(&self) -> &'static str {
        "reasoning.delta"
    }
}

/// Tool-use block has started.
#[derive(Debug, Clone, PartialEq)]
pub struct ToolUseStart {
    /// Tool-use identifier.
    pub id: String,
    /// Tool name.
    pub name: String,
}

impl StreamEvent for ToolUseStart {
    fn event_type(&self) -> &'static str {
        "tool_use.start"
    }
}

/// Incremental tool-use input delta.
#[derive(Debug, Clone, PartialEq)]
pub struct ToolUseDelta {
    /// Tool-use identifier.
    pub id: String,
    /// Incremental JSON argument fragment.
    pub input_delta: String,
}

impl StreamEvent for ToolUseDelta {
    fn event_type(&self) -> &'static str {
        "tool_use.delta"
    }
}

/// Tool-use block has stopped.
#[derive(Debug, Clone, PartialEq)]
pub struct ToolUseStop {
    /// Tool-use identifier.
    pub id: String,
}

impl StreamEvent for ToolUseStop {
    fn event_type(&self) -> &'static str {
        "tool_use.stop"
    }
}

/// Message has stopped.
#[derive(Debug, Clone, PartialEq)]
pub struct MessageStop {
    /// Finish reason.
    pub finish_reason: FinishReason,
}

impl StreamEvent for MessageStop {
    fn event_type(&self) -> &'static str {
        "message.stop"
    }
}

/// Token usage delta/update.
#[derive(Debug, Clone, PartialEq)]
pub struct UsageDelta {
    /// Usage counters.
    pub usage: Usage,
}

impl StreamEvent for UsageDelta {
    fn event_type(&self) -> &'static str {
        "usage.delta"
    }
}

/// Terminal stream error.
#[derive(Debug, Clone, PartialEq)]
pub struct ErrorEvent {
    /// Error message.
    pub message: String,
    /// Stable error code, when available.
    pub code: Option<String>,
}

impl StreamEvent for ErrorEvent {
    fn event_type(&self) -> &'static str {
        "error"
    }
}

impl ContentPart {
    /// Rough character estimate used by approximate token counters.
    #[must_use]
    pub fn approx_chars(&self) -> usize {
        match self {
            Self::Text { text } => text.len(),
            Self::ToolUse { input, .. } => {
                serde_json::Value::Object(input.clone()).to_string().len()
            }
            Self::ToolResult { content, .. } => content.len(),
            Self::Image { .. } | Self::Audio { .. } | Self::Video { .. } | Self::File { .. } => 256,
        }
    }
}

/// Wrap text in a single text content block.
#[must_use]
pub fn text_content(text: impl Into<String>) -> Vec<ContentPart> {
    vec![ContentPart::Text { text: text.into() }]
}

/// Extract concatenated text from content blocks.
#[must_use]
pub fn text_of(blocks: &[ContentPart]) -> String {
    blocks
        .iter()
        .filter_map(|block| match block {
            ContentPart::Text { text } => Some(text.as_str()),
            _ => None,
        })
        .collect::<Vec<_>>()
        .join("")
}

pub use prompt::{
    Builder, PromptError, PromptIdentity, PromptTemplate, Registry, RenderContext, RenderToMessage,
    Template, ValidationFinding, ValidationFindingKind, VariableDecl, VariableType, render,
    validate,
};

/// Typed AI error sentinels.
#[derive(Debug, Clone, PartialEq, Eq, Error)]
#[non_exhaustive]
pub enum GenAiError {
    /// Provider rate limit was hit.
    #[error("rate limited")]
    RateLimited,
    /// Request exceeded model context length.
    #[error("context length exceeded")]
    ContextLengthExceeded,
    /// Content filter rejected the request or response.
    #[error("content filtered")]
    ContentFilter,
    /// Model is overloaded.
    #[error("model overloaded")]
    ModelOverloaded,
    /// Budget was exceeded.
    #[error("budget exceeded: {0:?}")]
    BudgetExceeded(BudgetExceededReason),
    /// Requested model was not found.
    #[error("model not found")]
    ModelNotFound,
    /// Request was invalid.
    #[error("invalid request: {0}")]
    InvalidRequest(String),
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn content_part_serializes_tool_result_alias() {
        let block = ContentPart::ToolResult {
            id: "call-1".into(),
            content: "ok".into(),
            is_error: false,
        };
        let json = serde_json::to_value(&block).unwrap();
        assert_eq!(json["type"], "tool_result");
        assert_eq!(json["id"], "call-1");

        let legacy = serde_json::json!({
            "type": "tool_result",
            "tool_use_id": "call-2",
            "content": "ok"
        });
        let decoded: ContentPart = serde_json::from_value(legacy).unwrap();
        assert!(matches!(decoded, ContentPart::ToolResult { id, .. } if id == "call-2"));
    }

    fn assert_stream_event<T: StreamEvent>() {}

    #[test]
    fn stream_event_types_implement_trait_and_report_locked_names() {
        assert_stream_event::<MessageStart>();
        assert_stream_event::<TextDelta>();
        assert_stream_event::<ReasoningDelta>();
        assert_stream_event::<ToolUseStart>();
        assert_stream_event::<ToolUseDelta>();
        assert_stream_event::<ToolUseStop>();
        assert_stream_event::<MessageStop>();
        assert_stream_event::<UsageDelta>();
        assert_stream_event::<ErrorEvent>();

        let events: [StreamEventRef; 9] = [
            std::sync::Arc::new(MessageStart {
                role: Role::Assistant,
                model: "model".into(),
                request_id: Some("req-1".into()),
            }),
            std::sync::Arc::new(TextDelta {
                text: "text".into(),
            }),
            std::sync::Arc::new(ReasoningDelta {
                text: "think".into(),
            }),
            std::sync::Arc::new(ToolUseStart {
                id: "call-1".into(),
                name: "search".into(),
            }),
            std::sync::Arc::new(ToolUseDelta {
                id: "call-1".into(),
                input_delta: "{\"q\"".into(),
            }),
            std::sync::Arc::new(ToolUseStop {
                id: "call-1".into(),
            }),
            std::sync::Arc::new(MessageStop {
                finish_reason: FinishReason::Stop,
            }),
            std::sync::Arc::new(UsageDelta {
                usage: Usage {
                    input_tokens: 1,
                    output_tokens: 2,
                    cached_tokens: 3,
                    reasoning_tokens: 4,
                },
            }),
            std::sync::Arc::new(ErrorEvent {
                message: "boom".into(),
                code: Some("provider_error".into()),
            }),
        ];
        let wire_names = events
            .into_iter()
            .map(|event| event.event_type().to_owned())
            .collect::<Vec<_>>();

        assert_eq!(
            wire_names,
            [
                "message.start",
                "text.delta",
                "reasoning.delta",
                "tool_use.start",
                "tool_use.delta",
                "tool_use.stop",
                "message.stop",
                "usage.delta",
                "error",
            ]
        );
    }

    #[test]
    fn provider_custom_round_trips() {
        let provider = Provider::Custom("private".into());
        let json = serde_json::to_string(&provider).unwrap();
        let decoded: Provider = serde_json::from_str(&json).unwrap();
        assert_eq!(decoded, provider);
    }

    #[test]
    fn model_serializes_capabilities_roundtrip() {
        let model = Model {
            name: "gpt-4o".into(),
            provider: Provider::OpenAI,
            version: Some("2024-08-06".into()),
            capabilities: Capabilities {
                streaming: true,
                vision: true,
                max_input_tokens: Some(128_000),
                ..Default::default()
            },
        };
        let json = serde_json::to_string(&model).unwrap();
        assert!(json.contains("max_input_tokens"));
        let decoded: Model = serde_json::from_str(&json).unwrap();
        assert_eq!(decoded, model);
    }

    #[test]
    fn budget_roundtrips_and_errors_are_typed() {
        let budget = Budget {
            max_tokens: Some(10),
            max_calls: Some(2),
            max_cost: None,
            wall_clock: Some(60),
        };
        let json = serde_json::to_string(&budget).unwrap();
        let decoded: Budget = serde_json::from_str(&json).unwrap();
        assert_eq!(decoded, budget);

        let err = GenAiError::BudgetExceeded(BudgetExceededReason::Tokens);
        assert_eq!(err.to_string(), "budget exceeded: Tokens");
    }

    #[test]
    fn semconv_keys_and_operations_are_locked() {
        assert_eq!(
            [
                semconv::SYSTEM,
                semconv::OPERATION_NAME,
                semconv::REQUEST_ID,
                semconv::REQUEST_MODEL,
                semconv::REQUEST_MODEL_VERSION,
                semconv::REQUEST_MAX_TOKENS,
                semconv::REQUEST_TEMPERATURE,
                semconv::RESPONSE_MODEL,
                semconv::RESPONSE_FINISH_REASON,
                semconv::TOOL_NAME,
                semconv::USAGE_INPUT_TOKENS,
                semconv::USAGE_OUTPUT_TOKENS,
                semconv::USAGE_CACHED_TOKENS,
                semconv::USAGE_REASONING_TOKENS,
            ],
            [
                "gen_ai.system",
                "gen_ai.operation.name",
                "gen_ai.request.id",
                "gen_ai.request.model",
                "gen_ai.request.model.version",
                "gen_ai.request.max_tokens",
                "gen_ai.request.temperature",
                "gen_ai.response.model",
                "gen_ai.response.finish_reason",
                "gen_ai.tool.name",
                "gen_ai.usage.input_tokens",
                "gen_ai.usage.output_tokens",
                "gen_ai.usage.cached_tokens",
                "gen_ai.usage.reasoning_tokens",
            ]
        );
        let operations = [
            (semconv::Operation::Chat, "chat"),
            (semconv::Operation::TextCompletion, "text_completion"),
            (semconv::Operation::Embedding, "embeddings"),
            (semconv::Operation::AgentTurn, "agent.turn"),
            (semconv::Operation::LlmCall, "llm.call"),
            (semconv::Operation::ToolCall, "tool.call"),
            (semconv::Operation::McpRequest, "mcp.request"),
            (semconv::Operation::Stream, "stream"),
            (semconv::Operation::InferenceRequest, "inference.request"),
        ];
        for (operation, name) in operations {
            assert_eq!(operation.as_str(), name);
            assert_eq!(
                semconv::Operation::from_operation_name(name),
                Some(operation)
            );
        }
        assert_eq!(semconv::Operation::from_operation_name("predict"), None);
    }
}
