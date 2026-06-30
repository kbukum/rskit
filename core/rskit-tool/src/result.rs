//! Tool execution result types.

use rskit_ai::ToolResultBlock;
use serde::{Deserialize, Serialize};

use crate::io::{ToolMetadata, ToolOutput};

/// The outcome of executing a tool.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolResult {
    /// Structured JSON output (for programmatic consumption).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub output: Option<ToolOutput>,
    /// Human-readable text content.
    #[serde(default)]
    pub content: String,
    /// Whether the tool execution failed.
    #[serde(default)]
    pub is_error: bool,
    /// Arbitrary metadata attached to the result.
    #[serde(skip_serializing_if = "ToolMetadata::is_empty", default)]
    pub metadata: ToolMetadata,
}

impl ToolResult {
    /// Returns the text content of this result.
    pub fn text(&self) -> &str {
        &self.content
    }

    /// Build the `GenAI` tool-result block for sending to the model.
    pub fn to_block(&self, id: &str) -> ToolResultBlock {
        ToolResultBlock {
            id: id.to_string(),
            content: self.content.clone(),
            is_error: self.is_error,
        }
    }

    /// Insert a metadata key-value pair.
    pub fn set_meta(&mut self, key: &str, value: ToolOutput) {
        self.metadata.insert(key.to_string(), value);
    }
}

/// Create a success result with text content.
pub fn text_result(content: &str) -> ToolResult {
    ToolResult {
        output: None,
        content: content.to_string(),
        is_error: false,
        metadata: ToolMetadata::new(),
    }
}

/// Create an error result with text content.
pub fn error_result(content: &str) -> ToolResult {
    ToolResult {
        output: None,
        content: content.to_string(),
        is_error: true,
        metadata: ToolMetadata::new(),
    }
}

/// Create a success result by serializing a value to JSON.
pub fn json_result<T: Serialize>(value: &T) -> std::result::Result<ToolResult, serde_json::Error> {
    let content = serde_json::to_string(value)?;
    let json = serde_json::to_value(value)?;
    Ok(ToolResult {
        output: Some(ToolOutput::from(json)),
        content,
        is_error: false,
        metadata: ToolMetadata::new(),
    })
}
