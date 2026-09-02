use std::collections::BTreeMap;

use rskit_tool::ToolSchema;
use serde::{Deserialize, Serialize};

pub use rskit_ai::chat::{
    AssistantMessage, Message, SystemMessage, ToolResultMessage, UserMessage, assistant, system,
    tool_result_msg, user,
};
pub use rskit_ai::{
    ContentPart, FinishReason, ToolResultBlock, ToolUseBlock, Usage, text_content, text_of,
};

/// LLM-visible tool schema included with a completion request.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ToolDefinition {
    /// Stable tool identifier.
    pub name: String,
    /// Human-readable tool description.
    pub description: String,
    /// JSON Schema for the tool input shape.
    pub input_schema: ToolSchema,
    /// Optional JSON Schema for the tool output shape.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub output_schema: Option<ToolSchema>,
}

/// Controls how the model selects tools.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ToolChoice {
    /// Selection mode understood by the provider, such as `auto`, `none`, `required`, or `specific`.
    pub mode: String,
    /// Tool name to force when `mode` is `specific`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub function: Option<String>,
}

impl ToolChoice {
    /// Let the model decide whether to call a tool.
    #[must_use]
    pub fn auto() -> Self {
        Self {
            mode: "auto".to_string(),
            function: None,
        }
    }

    /// Disable tool calls for the request.
    #[must_use]
    pub fn none() -> Self {
        Self {
            mode: "none".to_string(),
            function: None,
        }
    }

    /// Require the model to call at least one available tool.
    #[must_use]
    pub fn required() -> Self {
        Self {
            mode: "required".to_string(),
            function: None,
        }
    }

    /// Require the model to call the named tool.
    #[must_use]
    pub fn specific(name: &str) -> Self {
        Self {
            mode: "specific".to_string(),
            function: Some(name.to_string()),
        }
    }
}

/// Request to generate a chat completion.
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
pub struct CompletionRequest {
    /// Provider model identifier to use for generation.
    pub model: String,
    /// Conversation messages sent to the provider.
    pub messages: Vec<Message>,
    /// Optional cap on generated tokens.
    pub max_tokens: Option<u32>,
    /// Optional sampling temperature.
    pub temperature: Option<f32>,
    /// Whether the provider should stream response events.
    pub stream: bool,
    /// Tools made available to the model for this request.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tools: Option<Vec<ToolDefinition>>,
    /// Optional policy controlling how the model may select tools.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tool_choice: Option<ToolChoice>,
    /// Optional system prompt applied ahead of the conversation.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub system_prompt: Option<String>,
    /// Optional nucleus-sampling probability mass.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub top_p: Option<f32>,
    /// Optional stop sequences that halt generation.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub stop_sequences: Vec<String>,
    /// Caller-supplied metadata for request tracking and observability.
    ///
    /// Not serialized to the provider wire; use [`Self::extra`] for
    /// provider-native metadata fields.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub metadata: BTreeMap<String, String>,
    /// Opaque provider-specific extension fields passed through untouched.
    #[serde(default, skip_serializing_if = "serde_json::Map::is_empty")]
    pub extra: serde_json::Map<String, serde_json::Value>,
}

/// Response from a chat completion.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CompletionResponse {
    /// Assistant message produced by the provider.
    pub message: AssistantMessage,
    /// Provider model identifier that generated the response.
    pub model: String,
    /// Token usage reported by the provider.
    pub usage: Usage,
    /// Provider finish reason, when supplied.
    pub stop_reason: Option<FinishReason>,
}

impl CompletionResponse {
    /// Returns true if the response contains tool call requests.
    #[must_use]
    pub fn has_tool_calls(&self) -> bool {
        self.message.has_tool_calls()
    }

    /// Extract text content from the response message.
    #[must_use]
    pub fn text(&self) -> String {
        self.message.text()
    }
}
