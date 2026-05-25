//! Streaming response event vocabulary.

use serde::{Deserialize, Serialize};

use crate::role::Role;
use crate::usage::Usage;

/// Canonical finish reason for GenAI responses.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
#[non_exhaustive]
pub enum FinishReason {
    /// Natural stop.
    Stop,
    /// Length/token limit reached.
    Length,
    /// Model stopped to request tool use.
    ToolUse,
    /// Provider content filter stopped generation.
    ContentFilter,
    /// Error stopped generation.
    Error,
    /// Cancellation stopped generation.
    Cancelled,
}

/// Incremental event emitted during an AI stream.
pub trait StreamEvent: Send + Sync + std::fmt::Debug {
    /// Stable event type emitted on the wire.
    fn event_type(&self) -> &'static str;
}

/// Shared stream event reference used across async boundaries.
pub type StreamEventRef = std::sync::Arc<dyn StreamEvent>;

/// Signals the start of a new message.
#[derive(Debug, Clone, PartialEq)]
pub struct MessageStart {
    /// Message role.
    pub role: Role,
    /// Model identifier.
    pub model: String,
    /// Provider request identifier, when available.
    pub request_id: Option<String>,
}

impl StreamEvent for MessageStart {
    fn event_type(&self) -> &'static str {
        "message.start"
    }
}

/// Incremental text delta.
#[derive(Debug, Clone, PartialEq)]
pub struct TextDelta {
    /// Text delta.
    pub text: String,
}

impl StreamEvent for TextDelta {
    fn event_type(&self) -> &'static str {
        "text.delta"
    }
}

/// Incremental reasoning delta.
#[derive(Debug, Clone, PartialEq)]
pub struct ReasoningDelta {
    /// Reasoning text delta.
    pub text: String,
}

impl StreamEvent for ReasoningDelta {
    fn event_type(&self) -> &'static str {
        "reasoning.delta"
    }
}

/// Tool-use block has started.
#[derive(Debug, Clone, PartialEq)]
pub struct ToolUseStart {
    /// Tool-use identifier.
    pub id: String,
    /// Tool name.
    pub name: String,
}

impl StreamEvent for ToolUseStart {
    fn event_type(&self) -> &'static str {
        "tool_use.start"
    }
}

/// Incremental tool-use input delta.
#[derive(Debug, Clone, PartialEq)]
pub struct ToolUseDelta {
    /// Tool-use identifier.
    pub id: String,
    /// Incremental JSON argument fragment.
    pub input_delta: String,
}

impl StreamEvent for ToolUseDelta {
    fn event_type(&self) -> &'static str {
        "tool_use.delta"
    }
}

/// Tool-use block has stopped.
#[derive(Debug, Clone, PartialEq)]
pub struct ToolUseStop {
    /// Tool-use identifier.
    pub id: String,
}

impl StreamEvent for ToolUseStop {
    fn event_type(&self) -> &'static str {
        "tool_use.stop"
    }
}

/// Message has stopped.
#[derive(Debug, Clone, PartialEq)]
pub struct MessageStop {
    /// Finish reason.
    pub finish_reason: FinishReason,
}

impl StreamEvent for MessageStop {
    fn event_type(&self) -> &'static str {
        "message.stop"
    }
}

/// Token usage delta/update.
#[derive(Debug, Clone, PartialEq)]
pub struct UsageDelta {
    /// Usage counters.
    pub usage: Usage,
}

impl StreamEvent for UsageDelta {
    fn event_type(&self) -> &'static str {
        "usage.delta"
    }
}

/// Terminal stream error.
#[derive(Debug, Clone, PartialEq)]
pub struct ErrorEvent {
    /// Error message.
    pub message: String,
    /// Stable error code, when available.
    pub code: Option<String>,
}

impl StreamEvent for ErrorEvent {
    fn event_type(&self) -> &'static str {
        "error"
    }
}
