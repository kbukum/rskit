//! Chat-completion-specific AI types.

pub mod message;
pub mod tokens;

pub use message::{
    AssistantMessage, Message, SystemMessage, ToolResultMessage, UserMessage, assistant, system,
    tool_result_msg, user,
};
pub use tokens::count_tokens_approx;
