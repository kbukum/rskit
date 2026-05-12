//! Prompt template types and validation.

use std::collections::{BTreeMap, BTreeSet};

use semver::Version;
use serde::{Deserialize, Serialize};
use thiserror::Error;

use super::render::{placeholders, render};

/// Errors returned by prompt rendering, construction, and registry lookup.
#[derive(Debug, Error)]
#[non_exhaustive]
pub enum PromptError {
    /// A named variable required by the template was absent from the render context.
    #[error("missing prompt variable {0:?}")]
    MissingVariable(String),
    /// A builder was missing a required field.
    #[error("missing required prompt field {0}")]
    MissingField(&'static str),
    /// A prompt version string was not valid semver.
    #[error("invalid prompt version {version:?}: {source}")]
    InvalidVersion {
        /// Supplied version.
        version: String,
        /// Parse failure.
        #[source]
        source: semver::Error,
    },
    /// A registry entry already exists.
    #[error("prompt already registered: {name}@{version}")]
    AlreadyRegistered {
        /// Prompt name.
        name: String,
        /// Prompt version.
        version: Version,
    },
    /// A registry entry was not found.
    #[error("prompt not found: {name}@{version}")]
    NotFound {
        /// Prompt name.
        name: String,
        /// Prompt version.
        version: Version,
    },
    /// No versions exist for a prompt name.
    #[error("prompt not found: {0}")]
    NameNotFound(String),
}

/// Render context for a prompt template.
pub type RenderContext = BTreeMap<String, serde_json::Value>;

/// Declared prompt variable type.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
#[non_exhaustive]
pub enum VariableType {
    /// String value.
    String,
    /// Number value.
    Number,
    /// Boolean value.
    Boolean,
    /// Object value.
    Object,
    /// Array value.
    Array,
    /// Any JSON value.
    #[default]
    Any,
}

/// Typed declaration for a prompt variable.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct VariableDecl {
    /// Variable name used in `{{name}}` placeholders.
    pub name: String,
    /// Expected variable type.
    #[serde(default, rename = "type")]
    pub kind: VariableType,
    /// Whether callers must supply the variable.
    #[serde(default = "default_required")]
    pub required: bool,
    /// Optional default value used when rendering.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub default: Option<serde_json::Value>,
}

const fn default_required() -> bool {
    true
}

/// Prompt template with semver identity and optional output schema.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PromptTemplate {
    /// Stable prompt name.
    pub name: String,
    /// Semver prompt version.
    pub version: Version,
    /// Template body. Variables use `{{name}}` placeholders.
    pub template: String,
    /// Declared input variables.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub variables: Vec<VariableDecl>,
    /// Optional output contract.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub output_schema: Option<serde_json::Value>,
    /// Human-readable description.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub description: String,
}

impl PromptTemplate {
    /// Render the template using the supplied context.
    pub fn render(&self, context: &RenderContext) -> Result<String, PromptError> {
        let mut values = context.clone();
        for variable in &self.variables {
            if !values.contains_key(&variable.name) {
                if let Some(default) = &variable.default {
                    values.insert(variable.name.clone(), default.clone());
                } else if variable.required {
                    return Err(PromptError::MissingVariable(variable.name.clone()));
                }
            }
        }
        render(&self.template, &values)
    }
}

/// Backwards-compatible template name for the canonical prompt template shape.
pub type Template = PromptTemplate;

/// Validation finding kind.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ValidationFindingKind {
    /// Placeholder has no declaration.
    MissingVariable,
    /// Declaration is not used by the template.
    UnusedVariable,
}

/// Prompt template validation finding.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ValidationFinding {
    /// Finding kind.
    pub kind: ValidationFindingKind,
    /// Variable name.
    pub variable: String,
}

/// Validate template placeholders against declarations.
#[must_use]
pub fn validate(template: &PromptTemplate) -> Vec<ValidationFinding> {
    let used = placeholders(&template.template);
    let declared = template
        .variables
        .iter()
        .map(|variable| variable.name.clone())
        .collect::<BTreeSet<_>>();
    let mut findings = Vec::new();
    for variable in used.difference(&declared) {
        findings.push(ValidationFinding {
            kind: ValidationFindingKind::MissingVariable,
            variable: variable.clone(),
        });
    }
    for variable in declared.difference(&used) {
        findings.push(ValidationFinding {
            kind: ValidationFindingKind::UnusedVariable,
            variable: variable.clone(),
        });
    }
    findings
}
