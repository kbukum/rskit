//! Tool execution result types.

use rskit_ai::ToolResultBlock;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// The outcome of executing a tool.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolResult {
    /// Structured JSON output (for programmatic consumption).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub output: Option<serde_json::Value>,
    /// Human-readable text content.
    #[serde(default)]
    pub content: String,
    /// Whether the tool execution failed.
    #[serde(default)]
    pub is_error: bool,
    /// Arbitrary metadata attached to the result.
    #[serde(skip_serializing_if = "HashMap::is_empty", default)]
    pub metadata: HashMap<String, serde_json::Value>,
}

impl ToolResult {
    /// Returns the text content of this result.
    pub fn text(&self) -> &str {
        &self.content
    }

    /// Build the GenAI tool-result block for sending to the model.
    pub fn to_block(&self, id: &str) -> ToolResultBlock {
        ToolResultBlock {
            id: id.to_string(),
            content: self.content.clone(),
            is_error: self.is_error,
        }
    }

    /// Insert a metadata key-value pair.
    pub fn set_meta(&mut self, key: &str, value: serde_json::Value) {
        self.metadata.insert(key.to_string(), value);
    }
}

/// Create a success result with text content.
pub fn text_result(content: &str) -> ToolResult {
    ToolResult {
        output: None,
        content: content.to_string(),
        is_error: false,
        metadata: HashMap::new(),
    }
}

/// Create an error result with text content.
pub fn error_result(content: &str) -> ToolResult {
    ToolResult {
        output: None,
        content: content.to_string(),
        is_error: true,
        metadata: HashMap::new(),
    }
}

/// Create a success result by serializing a value to JSON.
pub fn json_result<T: Serialize>(value: &T) -> std::result::Result<ToolResult, serde_json::Error> {
    let json = serde_json::to_value(value)?;
    let content = serde_json::to_string(value)?;
    Ok(ToolResult {
        output: Some(json),
        content,
        is_error: false,
        metadata: HashMap::new(),
    })
}
