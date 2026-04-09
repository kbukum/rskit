mod traits;

/// LLM message types, request/response structs, and helper constructors.
pub mod types;

pub use traits::LlmProvider;
pub use types::{
    AssistantMessage, CompletionRequest, CompletionResponse, ContentBlock, FunctionCall, Message,
    StopReason, SystemMessage, ToolCall, ToolChoice, ToolResultMessage, Usage, UserMessage,
    assistant, system, text_content, text_of, tool_result_msg, user,
};
