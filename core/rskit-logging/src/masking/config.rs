//! Configuration for the masking engine.

use serde::Deserialize;

/// Configuration for sensitive data masking.
#[derive(Debug, Clone, Deserialize)]
pub struct MaskingConfig {
    /// Whether masking is enabled.  Default: `true`.
    #[serde(default = "default_true")]
    pub enabled: bool,

    /// Additional field names to mask (beyond defaults).
    #[serde(default)]
    pub field_names: Vec<String>,

    /// Additional regex patterns for value masking.
    #[serde(default)]
    pub value_patterns: Vec<String>,

    /// Replacement string for field-name masking.  Default: `"[REDACTED]"`.
    #[serde(default = "default_replacement")]
    pub replacement: String,
}

fn default_true() -> bool {
    true
}

fn default_replacement() -> String {
    "[REDACTED]".to_string()
}

impl Default for MaskingConfig {
    fn default() -> Self {
        Self {
            enabled: default_true(),
            field_names: Vec::new(),
            value_patterns: Vec::new(),
            replacement: default_replacement(),
        }
    }
}
