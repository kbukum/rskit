//! Canonical AI chat message types.

use serde::{Deserialize, Serialize};

use crate::{ContentPart, ToolUseBlock, Usage, text_content, text_of};

/// A chat message discriminated by role.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "role", rename_all = "snake_case")]
pub enum Message {
    /// User-authored message.
    User(UserMessage),
    /// Assistant/model-authored message.
    Assistant(AssistantMessage),
    /// System instruction.
    System(SystemMessage),
    /// Tool execution result.
    #[serde(rename = "tool_result", alias = "tool")]
    Tool(ToolResultMessage),
}

impl Message {
    /// Return the wire role string.
    #[must_use]
    pub fn role(&self) -> &'static str {
        match self {
            Self::User(_) => "user",
            Self::Assistant(_) => "assistant",
            Self::System(_) => "system",
            Self::Tool(_) => "tool_result",
        }
    }
}

/// A user message.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
pub struct UserMessage {
    /// Content parts supplied by the user.
    pub content: Vec<ContentPart>,
}

impl UserMessage {
    /// Construct a single-text user message.
    #[must_use]
    pub fn from_text(text: impl Into<String>) -> Self {
        Self {
            content: text_content(text),
        }
    }
}

/// An assistant message.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
pub struct AssistantMessage {
    /// Assistant content parts.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub content: Vec<ContentPart>,
    /// Canonical tool-use blocks requested by the assistant.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub tool_calls: Vec<ToolUseBlock>,
    /// Per-turn usage when tracked in conversation history.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub usage: Option<Usage>,
}

impl AssistantMessage {
    /// Concatenate all text content parts.
    #[must_use]
    pub fn text(&self) -> String {
        text_of(&self.content)
    }

    /// Whether this message requested any tool calls.
    #[must_use]
    pub fn has_tool_calls(&self) -> bool {
        !self.tool_calls.is_empty()
    }
}

/// A system instruction message.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SystemMessage {
    /// System prompt content.
    pub content: String,
}

/// A tool execution result message.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ToolResultMessage {
    /// Tool-use identifier satisfied by this result.
    #[serde(alias = "tool_use_id")]
    pub id: String,
    /// Human-readable tool output.
    pub content: String,
    /// Whether the tool execution failed.
    #[serde(default)]
    pub is_error: bool,
}

/// Create a user message with text content.
#[must_use]
pub fn user(text: &str) -> Message {
    Message::User(UserMessage::from_text(text))
}

/// Create an assistant message with text content.
#[must_use]
pub fn assistant(text: &str) -> Message {
    Message::Assistant(AssistantMessage {
        content: text_content(text),
        tool_calls: Vec::new(),
        usage: None,
    })
}

/// Create a system message.
#[must_use]
pub fn system(text: &str) -> Message {
    Message::System(SystemMessage {
        content: text.to_string(),
    })
}

/// Create a tool-result message.
#[must_use]
pub fn tool_result_msg(tool_use_id: &str, content: &str, is_error: bool) -> Message {
    Message::Tool(ToolResultMessage {
        id: tool_use_id.to_string(),
        content: content.to_string(),
        is_error,
    })
}
