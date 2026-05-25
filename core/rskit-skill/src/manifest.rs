//! Skill manifest schema and validation.

use rskit_validation::Validator;
use rskit_validation::input::validate_safe_path;
use serde::{Deserialize, Serialize};

use crate::SkillError;

/// Skill safety order. Informational in manifests; effective safety is computed from tools.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize, Default)]
#[serde(rename_all = "kebab-case")]
pub enum Safety {
    /// Read-only skill intent.
    #[default]
    ReadOnly,
    /// Mutating skill intent.
    Mutating,
    /// Destructive skill intent.
    Destructive,
}

/// Locked skill manifest.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Manifest {
    /// Canonical manifest schema version.
    #[serde(rename = "schema_version")]
    pub schema_version: String,
    /// Stable skill name.
    pub name: String,
    /// Semantic version.
    pub version: String,
    /// Human-readable description.
    pub description: String,
    /// Optional license expression.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub license: Option<String>,
    /// Optional authors.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub authors: Vec<String>,
    /// Referenced tools, resources, prompts, and MCP servers.
    pub references: References,
    /// Activation requirements.
    #[serde(default)]
    pub requires: Requires,
    /// Human approval checkpoints independent from tool sensitive invocations.
    #[serde(
        default,
        rename = "human_approval",
        skip_serializing_if = "Vec::is_empty"
    )]
    pub human_approval: Vec<HumanApprovalStep>,
    /// Optional budgets requested by the skill.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub budgets: Option<Budgets>,
    /// Optional model routing hints.
    #[serde(
        default,
        rename = "model_hints",
        skip_serializing_if = "Option::is_none"
    )]
    pub model_hints: Option<ModelHints>,
    /// Progressive disclosure text.
    #[serde(
        default,
        rename = "progressive_disclosure",
        skip_serializing_if = "Option::is_none"
    )]
    pub progressive_disclosure: Option<ProgressiveDisclosure>,
    /// Inert script assets.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub scripts: Vec<ScriptAsset>,
    /// Optional signature metadata.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub signature: Option<Signature>,
    /// Informational declared safety; does not grant authority.
    pub safety: Safety,
}

impl Manifest {
    /// Validate required string fields.
    pub fn validate(&self) -> Result<(), SkillError> {
        Validator::new()
            .required("schema_version", &self.schema_version)
            .required("name", &self.name)
            .required("version", &self.version)
            .required("description", &self.description)
            .validate()
            .map_err(|error| SkillError::InvalidManifest(error.to_string()))?;

        for script in &self.scripts {
            validate_safe_path(&script.path)
                .map_err(|error| SkillError::InvalidManifest(error.to_string()))?;
        }
        Ok(())
    }
}

/// Prompt reference with explicit version.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PromptRef {
    /// Prompt name.
    pub name: String,
    /// Prompt version.
    pub version: String,
}

/// References to executable and context-bearing registrations by name/pattern.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct References {
    /// Tool names referenced by the skill.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub tools: Vec<String>,
    /// Prompt names and versions referenced by the skill.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub prompts: Vec<PromptRef>,
    /// Resource URI patterns referenced by the skill.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub resources: Vec<String>,
    /// MCP server names referenced by the skill.
    #[serde(default, rename = "mcp_servers", skip_serializing_if = "Vec::is_empty")]
    pub mcp_servers: Vec<String>,
}

/// Activation preconditions.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Requires {
    /// Scopes the principal must hold to activate the skill. These never grant executable authority.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub scopes: Vec<String>,
    /// Capability gates such as network or filesystem.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub capabilities: Vec<String>,
}

/// Human approval checkpoint.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct HumanApprovalStep {
    /// Workflow step name.
    pub step: String,
    /// Human-readable condition.
    pub when: String,
    /// Why approval is required.
    pub rationale: String,
}

/// Skill-requested budget limits.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Budgets {
    /// Maximum tokens.
    #[serde(
        default,
        rename = "max_tokens",
        skip_serializing_if = "Option::is_none"
    )]
    pub max_tokens: Option<u64>,
    /// Maximum calls.
    #[serde(default, rename = "max_calls", skip_serializing_if = "Option::is_none")]
    pub max_calls: Option<u32>,
    /// Maximum cost.
    #[serde(default, rename = "max_cost", skip_serializing_if = "Option::is_none")]
    pub max_cost: Option<MaxCost>,
    /// ISO 8601 wall-clock duration.
    #[serde(
        default,
        rename = "wall_clock",
        skip_serializing_if = "Option::is_none"
    )]
    pub wall_clock: Option<String>,
}

/// Maximum cost budget.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MaxCost {
    /// Decimal amount.
    pub amount: f64,
    /// Currency code.
    pub currency: String,
}

/// Optional model hints.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ModelHints {
    /// Ordered list of preferred model identifiers.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub preferred: Vec<String>,
    /// Model identifiers to reject.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub reject: Vec<String>,
}

/// Signature metadata carried by the manifest.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Signature {
    /// Signature algorithm or verifier hint.
    pub algorithm: String,
    /// Signature value.
    pub value: String,
    /// Verifier key identifier.
    #[serde(rename = "key_id")]
    pub key_id: String,
}

/// Progressive disclosure copy.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ProgressiveDisclosure {
    /// Short summary.
    pub summary: String,
    /// Detailed disclosure text.
    pub detail: String,
}

/// Inert script asset metadata.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ScriptAsset {
    /// Script path relative to pack root.
    pub path: String,
    /// Script description.
    pub description: String,
}
