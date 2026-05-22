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
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
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
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ToolChoice {
    pub mode: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub function: Option<String>,
}

impl ToolChoice {
    #[must_use]
    pub fn auto() -> Self {
        Self {
            mode: "auto".to_string(),
            function: None,
        }
    }

    #[must_use]
    pub fn none() -> Self {
        Self {
            mode: "none".to_string(),
            function: None,
        }
    }

    #[must_use]
    pub fn required() -> Self {
        Self {
            mode: "required".to_string(),
            function: None,
        }
    }

    #[must_use]
    pub fn specific(name: &str) -> Self {
        Self {
            mode: "specific".to_string(),
            function: Some(name.to_string()),
        }
    }
}

/// Request to generate a chat completion.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CompletionRequest {
    pub model: String,
    pub messages: Vec<Message>,
    pub max_tokens: Option<u32>,
    pub temperature: Option<f32>,
    pub stream: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tools: Option<Vec<ToolDefinition>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tool_choice: Option<ToolChoice>,
}

/// Response from a chat completion.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CompletionResponse {
    pub message: AssistantMessage,
    pub model: String,
    pub usage: Usage,
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
