//! Executable permission envelope for tools.

use serde::{Deserialize, Serialize};

/// Executable permission envelope declared by a tool.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Envelope {
    /// Authz scopes consumed by this tool. Empty means no named scope is required.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub scopes: Vec<String>,
    /// Network egress policy. Defaults to deny all egress.
    #[serde(default)]
    pub network: NetworkPolicy,
    /// Filesystem access rules. Empty denies filesystem access.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub filesystem: Vec<FilesystemRule>,
    /// Subprocess invocation rules. Empty denies subprocess execution.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub subprocess: Vec<SubprocessRule>,
    /// Safety classification for executable behavior.
    #[serde(default)]
    pub safety: Safety,
    /// Input predicates that require human approval before invocation.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub sensitive_invocations: Vec<SensitivePredicate>,
    /// Informational data classification used for redaction/observability.
    #[serde(default)]
    pub data_classification: DataClassification,
}

impl Default for Envelope {
    fn default() -> Self {
        Self {
            scopes: Vec::new(),
            network: NetworkPolicy::None,
            filesystem: Vec::new(),
            subprocess: Vec::new(),
            safety: Safety::ReadOnly,
            sensitive_invocations: Vec::new(),
            data_classification: DataClassification::Public,
        }
    }
}

/// Network egress policy for a tool.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(tag = "mode", rename_all = "snake_case")]
pub enum NetworkPolicy {
    /// Deny all network egress.
    #[default]
    None,
    /// Allow only these host rules.
    AllowList {
        /// Host allow-list. Hosts match exact names, exact IP literals, or exact suffix after leading dot.
        rules: Vec<NetworkRule>,
    },
}

/// Single network allow-list rule.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct NetworkRule {
    /// Exact host, IP literal, or suffix beginning with `.`.
    pub host: String,
    /// Optional TCP port.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub port: Option<u16>,
    /// Optional scheme. Defaults to `https` when absent.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub scheme: Option<String>,
}

/// Filesystem access rule.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FilesystemRule {
    /// Normalized root path or explicit glob.
    pub path: String,
    /// Access mode allowed under `path`.
    pub mode: FilesystemMode,
}

/// Filesystem access mode.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FilesystemMode {
    /// Read files.
    Read,
    /// Write files.
    Write,
    /// Delete files.
    Delete,
}

/// Subprocess execution rule.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SubprocessRule {
    /// Argv pattern. First element must match exactly; later elements may be literals or `{}` placeholders.
    pub argv_pattern: Vec<String>,
    /// Environment variable names allowed to pass through. Empty by default.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub env_allow: Vec<String>,
    /// Optional normalized working directory.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cwd: Option<String>,
}

/// Total safety order for executable behavior.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize, Default)]
#[serde(rename_all = "kebab-case")]
pub enum Safety {
    /// Read-only behavior.
    #[default]
    ReadOnly,
    /// Mutating behavior.
    Mutating,
    /// Destructive behavior.
    Destructive,
}

/// Informational data classification.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum DataClassification {
    /// Public data.
    #[default]
    Public,
    /// Personally identifiable information.
    Pii,
    /// Secret or credential-bearing data.
    Secret,
}

/// Predicate over validated input that marks an invocation as sensitive.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SensitivePredicate {
    /// JSONPath-like selector over the tool input.
    pub jsonpath: String,
    /// Matcher applied to the selected value.
    pub matcher: SensitiveMatcher,
}

/// Matcher for a sensitive invocation predicate.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", content = "value", rename_all = "snake_case")]
pub enum SensitiveMatcher {
    /// The selected value exists.
    Exists,
    /// The selected value equals this JSON value.
    Equals(serde_json::Value),
    /// The selected string matches this regular expression.
    Regex(String),
    /// The selected numeric value is greater than this threshold.
    Gt(f64),
    /// The selected numeric value is less than this threshold.
    Lt(f64),
}
