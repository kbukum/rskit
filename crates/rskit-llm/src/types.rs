use rskit_tool::Definition;
use serde::{Deserialize, Serialize};

// ── Content Blocks ──────────────────────────────────────────────────────────

/// A single block of content within a message.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum ContentBlock {
    #[serde(rename = "text")]
    Text { text: String },
    #[serde(rename = "image")]
    Image {
        source: String,
        mime_type: String,
        #[serde(skip_serializing_if = "Option::is_none")]
        data: Option<String>,
    },
    #[serde(rename = "tool_use")]
    ToolUse {
        id: String,
        name: String,
        input: serde_json::Value,
    },
    #[serde(rename = "tool_result")]
    ToolResult {
        tool_use_id: String,
        content: String,
        #[serde(default)]
        is_error: bool,
    },
    #[serde(rename = "thinking")]
    Thinking { text: String },
}

// ── Message variants ────────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UserMessage {
    pub content: Vec<ContentBlock>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AssistantMessage {
    #[serde(default)]
    pub content: Vec<ContentBlock>,
    #[serde(default)]
    pub tool_calls: Vec<ToolCall>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub usage: Option<Usage>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolResultMessage {
    pub tool_use_id: String,
    pub content: String,
    #[serde(default)]
    pub is_error: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SystemMessage {
    pub content: String,
}

// ── Discriminated union ─────────────────────────────────────────────────────

/// A chat message — discriminated by role.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "role")]
pub enum Message {
    #[serde(rename = "user")]
    User(UserMessage),
    #[serde(rename = "assistant")]
    Assistant(AssistantMessage),
    #[serde(rename = "tool_result")]
    ToolResult(ToolResultMessage),
    #[serde(rename = "system")]
    System(SystemMessage),
}

impl Message {
    pub fn role(&self) -> &str {
        match self {
            Message::User(_) => "user",
            Message::Assistant(_) => "assistant",
            Message::ToolResult(_) => "tool_result",
            Message::System(_) => "system",
        }
    }
}

// ── Convenience constructors ────────────────────────────────────────────────

/// Create a user message with text content.
pub fn user(text: &str) -> Message {
    Message::User(UserMessage {
        content: vec![ContentBlock::Text {
            text: text.to_string(),
        }],
    })
}

/// Create an assistant message with text content.
pub fn assistant(text: &str) -> Message {
    Message::Assistant(AssistantMessage {
        content: vec![ContentBlock::Text {
            text: text.to_string(),
        }],
        tool_calls: vec![],
        usage: None,
    })
}

/// Create a system message.
pub fn system(text: &str) -> Message {
    Message::System(SystemMessage {
        content: text.to_string(),
    })
}

/// Create a tool-result message.
pub fn tool_result_msg(tool_use_id: &str, content: &str, is_error: bool) -> Message {
    Message::ToolResult(ToolResultMessage {
        tool_use_id: tool_use_id.to_string(),
        content: content.to_string(),
        is_error,
    })
}

/// Wrap text in a single-element content block vector.
pub fn text_content(text: &str) -> Vec<ContentBlock> {
    vec![ContentBlock::Text {
        text: text.to_string(),
    }]
}

/// Extract all text from content blocks, joining them.
pub fn text_of(blocks: &[ContentBlock]) -> String {
    blocks
        .iter()
        .filter_map(|b| match b {
            ContentBlock::Text { text } => Some(text.as_str()),
            _ => None,
        })
        .collect::<Vec<_>>()
        .join("")
}

// ── Tool call (OpenAI-style) ────────────────────────────────────────────────

/// Function invocation details within a tool call.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FunctionCall {
    pub name: String,
    pub arguments: String,
}

/// An LLM's request to invoke a tool.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolCall {
    pub id: String,
    #[serde(rename = "type")]
    pub call_type: String,
    pub function: FunctionCall,
}

// ── Tool choice ─────────────────────────────────────────────────────────────

/// Controls how the model selects tools.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolChoice {
    pub mode: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub function: Option<String>,
}

impl ToolChoice {
    pub fn auto() -> Self {
        Self {
            mode: "auto".to_string(),
            function: None,
        }
    }

    pub fn none() -> Self {
        Self {
            mode: "none".to_string(),
            function: None,
        }
    }

    pub fn required() -> Self {
        Self {
            mode: "required".to_string(),
            function: None,
        }
    }

    pub fn specific(name: &str) -> Self {
        Self {
            mode: "specific".to_string(),
            function: Some(name.to_string()),
        }
    }
}

// ── Usage ───────────────────────────────────────────────────────────────────

/// Token usage statistics.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Usage {
    pub input_tokens: u32,
    pub output_tokens: u32,
}

// ── Stop reason ─────────────────────────────────────────────────────────────

/// Why the model stopped generating.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum StopReason {
    EndTurn,
    ToolUse,
    MaxTokens,
    ContentFilter,
    StopSequence,
}

// ── Stream chunk ────────────────────────────────────────────────────────────

/// A single parsed chunk from a streaming completion response.
///
/// Each provider dialect parses its wire format into this unified type so the
/// caller can accumulate content and tool-call fragments without knowing which
/// provider is behind the stream.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct StreamChunk {
    /// Incremental text content from this chunk (empty when absent).
    pub content: String,
    /// Tool-call fragments contained in this chunk.
    pub tool_calls: Vec<ToolCall>,
    /// `true` when the provider signals the stream is finished.
    pub done: bool,
}

// ── Request / Response ──────────────────────────────────────────────────────

/// Request to generate a chat completion.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CompletionRequest {
    pub model: String,
    pub messages: Vec<Message>,
    pub max_tokens: Option<u32>,
    pub temperature: Option<f32>,
    pub stream: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tools: Option<Vec<Definition>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tool_choice: Option<ToolChoice>,
}

/// Response from a chat completion.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CompletionResponse {
    pub message: AssistantMessage,
    pub model: String,
    pub usage: Usage,
    pub stop_reason: Option<StopReason>,
}

impl CompletionResponse {
    /// Returns true if the response contains tool call requests.
    pub fn has_tool_calls(&self) -> bool {
        !self.message.tool_calls.is_empty()
    }

    /// Extract text content from the response message.
    pub fn text(&self) -> String {
        text_of(&self.message.content)
    }
}
