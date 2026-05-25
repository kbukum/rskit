//! Multimodal content and tool block vocabulary.

use serde::{Deserialize, Serialize};

/// A single content part in a multimodal message.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
#[non_exhaustive]
pub enum ContentPart {
    /// Plain text content.
    Text {
        /// UTF-8 text.
        text: String,
    },
    /// Image content by URL/source and optional inline data.
    Image {
        /// Provider-readable source, such as a URL or media identifier.
        source: String,
        /// MIME type, for example `image/png`.
        mime_type: String,
        /// Optional base64-encoded data.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        data: Option<String>,
    },
    /// Audio content by URL/source and optional inline data.
    Audio {
        /// Provider-readable source, such as a URL or media identifier.
        source: String,
        /// MIME type, for example `audio/mpeg`.
        mime_type: String,
        /// Optional base64-encoded data.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        data: Option<String>,
    },
    /// Video content by URL/source and optional inline data.
    Video {
        /// Provider-readable source, such as a URL or media identifier.
        source: String,
        /// MIME type, for example `video/mp4`.
        mime_type: String,
        /// Optional base64-encoded data.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        data: Option<String>,
    },
    /// File content by URL/source and optional inline data.
    File {
        /// Provider-readable source, such as a URL or media identifier.
        source: String,
        /// MIME type, for example `application/pdf`.
        mime_type: String,
        /// Optional base64-encoded data.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        data: Option<String>,
    },
    /// Tool-use request block emitted by a model.
    ToolUse {
        /// Tool-use identifier.
        id: String,
        /// Tool name.
        name: String,
        /// Tool input JSON.
        input: serde_json::Map<String, serde_json::Value>,
    },
    /// Tool-result block returned to a model.
    ToolResult {
        /// Tool-use identifier this result satisfies.
        #[serde(alias = "tool_use_id")]
        id: String,
        /// Human-readable content.
        content: String,
        /// Whether the tool returned an error.
        #[serde(default)]
        is_error: bool,
    },
}

/// Tool-use block shape shared across LLM/tool/MCP boundaries.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ToolUseBlock {
    /// Tool-use identifier.
    pub id: String,
    /// Tool name.
    pub name: String,
    /// Tool input JSON.
    pub input: serde_json::Map<String, serde_json::Value>,
}

/// Tool-result block shape shared across LLM/tool/MCP boundaries.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ToolResultBlock {
    /// Tool-use identifier this result satisfies.
    pub id: String,
    /// Human-readable content returned by the tool.
    pub content: String,
    /// Whether the result represents a tool failure.
    #[serde(default)]
    pub is_error: bool,
}

impl ContentPart {
    /// Rough character estimate used by approximate token counters.
    #[must_use]
    pub fn approx_chars(&self) -> usize {
        match self {
            Self::Text { text } => text.len(),
            Self::ToolUse { input, .. } => {
                serde_json::Value::Object(input.clone()).to_string().len()
            }
            Self::ToolResult { content, .. } => content.len(),
            Self::Image { .. } | Self::Audio { .. } | Self::Video { .. } | Self::File { .. } => 256,
        }
    }
}

/// Wrap text in a single text content block.
#[must_use]
pub fn text_content(text: impl Into<String>) -> Vec<ContentPart> {
    vec![ContentPart::Text { text: text.into() }]
}

/// Extract concatenated text from content blocks.
#[must_use]
pub fn text_of(blocks: &[ContentPart]) -> String {
    blocks
        .iter()
        .filter_map(|block| match block {
            ContentPart::Text { text } => Some(text.as_str()),
            _ => None,
        })
        .collect::<Vec<_>>()
        .join("")
}
