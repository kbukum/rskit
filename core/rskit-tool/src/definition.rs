//! Tool definition types — MCP-aligned metadata.

use serde::{Deserialize, Serialize};

use crate::envelope::Envelope;

/// How the frontend should handle the tool result.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "lowercase")]
#[non_exhaustive]
pub enum ExecutionHint {
    /// Tool executes a real operation; result is authoritative.
    #[default]
    Backend,
    /// Tool only validates/extracts params; frontend drives the action.
    Ui,
    /// Tool executes backend AND frontend should refresh/navigate.
    Hybrid,
    /// Unknown hint from a newer protocol version; normalizes to Backend.
    #[serde(other)]
    Unknown,
}

impl Serialize for ExecutionHint {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        match self.effective() {
            Self::Backend | Self::Unknown => serializer.serialize_str("backend"),
            Self::Ui => serializer.serialize_str("ui"),
            Self::Hybrid => serializer.serialize_str("hybrid"),
        }
    }
}

impl ExecutionHint {
    /// Return the effective hint, mapping Unknown → Backend.
    #[must_use]
    pub fn effective(self) -> Self {
        match self {
            Self::Unknown => Self::Backend,
            other => other,
        }
    }
}

/// Optional hints about tool behavior (MCP-aligned).
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct Annotations {
    /// Human-readable title.
    #[serde(skip_serializing_if = "String::is_empty", default)]
    pub title: String,
    /// Grouping category for UI.
    #[serde(skip_serializing_if = "String::is_empty", default)]
    pub category: String,
    /// Freeform tags for filtering.
    #[serde(skip_serializing_if = "Vec::is_empty", default)]
    pub tags: Vec<String>,
    /// True if repeated calls produce the same result.
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub idempotent_hint: Option<bool>,
    /// Tells the frontend how to handle the tool result.
    #[serde(default)]
    pub execution_hint: ExecutionHint,
}

/// Describes a tool — MCP-aligned metadata.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Definition {
    /// Unique tool identifier.
    pub name: String,
    /// Human-readable description.
    pub description: String,
    /// JSON Schema for the tool's input parameters.
    pub input_schema: serde_json::Value,
    /// Optional JSON Schema for the tool's output.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub output_schema: Option<serde_json::Value>,
    /// Behavioral hints that are orthogonal to the executable permission envelope.
    #[serde(default)]
    pub annotations: Annotations,
    /// Executable permission envelope — the single source of truth for what
    /// the tool may do at runtime. It carries scopes, network/filesystem/
    /// subprocess rules, safety classification, sensitive-invocation
    /// predicates, and data-classification hints. Defaults deny network,
    /// filesystem, and subprocess access.
    #[serde(default)]
    pub envelope: Envelope,
}
