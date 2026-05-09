//! LLM provider abstractions for OpenAI, Anthropic, and other backends.
//!
//! Defines request/response structs, stream helpers, and the canonical
//! [`Provider`] trait shared across LLM provider implementations.

/// LLM request/response types and helper constructors.
pub mod types;

/// Streaming event types emitted during completion.
pub mod stream_events;

/// Provider trait with streaming, capabilities, and token counting.
pub mod provider;

/// Component lifecycle mixin for LLM providers (D12).
pub mod lifecycle;

pub use lifecycle::Lifecycle;
pub use provider::{LlmRequestResponse, LlmStream, Provider};
pub use rskit_ai::chat::{
    AssistantMessage, Message, SystemMessage, ToolResultMessage, UserMessage, assistant, system,
    tool_result_msg, user,
};
pub use rskit_ai::{
    Budget, BudgetExceededReason, Capabilities, ContentPart, Cost, ErrorEvent, FinishReason,
    GenAiError, MessageStart, MessageStop, Model, Money, ReasoningDelta, StreamEvent,
    StreamEventRef, TextDelta, ToolResultBlock, ToolUseBlock, ToolUseDelta, ToolUseStart,
    ToolUseStop, Usage, UsageDelta, text_content, text_of,
};
pub use types::{CompletionRequest, CompletionResponse, ToolChoice, ToolDefinition};
