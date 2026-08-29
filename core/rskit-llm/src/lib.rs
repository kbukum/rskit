//! LLM provider abstractions for `OpenAI`, `Anthropic`, and other backends.
//!
//! Defines request/response structs, stream helpers,
//! and the canonical [`Provider`] trait shared across LLM provider implementations.
//!
//! Also owns the canonical [`TokenCounter`] port and its dependency-free
//! [`HeuristicTokenCounter`] default; exact tokenizers ship as feature-gated
//! `contrib/llm/*` adapters implementing the same trait.
#![warn(missing_docs)]

/// LLM request/response types and helper constructors.
pub mod types;

/// Deterministic echo provider for local composition and downstream tests.
pub mod echo;

/// Streaming event types emitted during completion.
pub mod stream_events;

/// Provider trait with streaming, capabilities, and token counting.
pub mod provider;
/// Explicit LLM provider registry.
pub mod registry;

/// Component lifecycle mixin for LLM providers (D12).
pub mod lifecycle;

/// Token counting port and dependency-free heuristic default.
pub mod tokenizer;

pub use echo::Echo;
pub use lifecycle::Lifecycle;
pub use provider::{LlmRequestResponse, LlmStream, Provider};
pub use registry::{Factory, Registry, default_registry};
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
pub use tokenizer::{HeuristicTokenCounter, TokenCounter};
pub use types::{CompletionRequest, CompletionResponse, ToolChoice, ToolDefinition};
