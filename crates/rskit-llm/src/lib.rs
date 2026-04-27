//! LLM provider abstractions for OpenAI, Anthropic, and other backends.
//!
//! Defines message types, request/response structs, and streaming helpers that
//! are shared across LLM provider implementations.

mod traits;

/// LLM message types, request/response structs, and helper constructors.
pub mod types;

/// Streaming event types emitted during completion.
pub mod stream_events;

/// Provider trait with streaming, capabilities, and token counting.
pub mod provider;

pub use provider::{Capabilities, Provider, count_tokens_approx};
pub use stream_events::StreamEvent;
pub use traits::LlmProvider;
pub use types::{
    AssistantMessage, CompletionRequest, CompletionResponse, ContentBlock, FunctionCall, Message,
    StopReason, StreamChunk, SystemMessage, ToolCall, ToolChoice, ToolResultMessage, Usage,
    UserMessage, assistant, system, text_content, text_of, tool_result_msg, user,
};
