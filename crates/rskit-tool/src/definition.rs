//! Tool definition types — MCP-aligned metadata.

use serde::{Deserialize, Serialize};

/// Optional hints about tool behavior (MCP-aligned).
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Annotations {
    /// Human-readable title.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub title: Option<String>,
    /// True if the tool only reads data.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub read_only_hint: Option<bool>,
    /// True if the tool may cause irreversible changes.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub destructive_hint: Option<bool>,
    /// True if repeated calls produce the same result.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub idempotent_hint: Option<bool>,
    /// True if the tool interacts with external systems.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub open_world_hint: Option<bool>,
    /// Grouping category for UI.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub category: Option<String>,
    /// Freeform tags for filtering.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tags: Option<Vec<String>>,
}

/// Describes a tool — MCP-aligned metadata.
#[derive(Debug, Clone, Serialize, Deserialize)]
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
    /// Optional behavioral hints.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub annotations: Option<Annotations>,
    /// Whether the tool only reads data.
    #[serde(default)]
    pub read_only: bool,
    /// Whether the tool may cause irreversible changes.
    #[serde(default)]
    pub destructive: bool,
    /// Maximum result size in bytes (0 = unlimited).
    #[serde(default)]
    pub max_result_size: usize,
    /// Timeout in seconds (0 = no timeout).
    #[serde(default)]
    pub timeout_secs: f64,
}
