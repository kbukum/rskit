//! RBAC + ABAC policy model: subjects, resources, requests, roles, permissions, and conditions.

use std::collections::HashMap;

use serde::{Deserialize, Serialize};
use serde_json::Value;

/// Request, subject, or resource attributes.
pub type Attributes = HashMap<String, Value>;

/// The authenticated caller.
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
pub struct Subject {
    /// Subject identifier.
    pub id: String,
    /// RBAC role bindings.
    pub roles: Vec<String>,
    /// Additional subject attributes for ABAC evaluation.
    pub attributes: Attributes,
}

/// The target being accessed.
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
pub struct Resource {
    /// Resource type or namespace.
    pub resource_type: String,
    /// Resource identifier.
    pub id: String,
    /// Additional resource attributes for ABAC evaluation.
    pub attributes: Attributes,
}

/// Canonical authorization request.
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
pub struct Request {
    /// Subject performing the action.
    pub subject: Subject,
    /// Resource being accessed.
    pub resource: Resource,
    /// Requested action.
    pub action: String,
    /// Request-scoped attributes.
    pub context: Attributes,
}

/// RBAC permission with wildcard matching.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Permission {
    /// Resource pattern.
    pub resource: String,
    /// Action pattern.
    pub action: String,
    /// Attribute conditions that must hold for this permission grant.
    #[serde(default)]
    pub conditions: Vec<Condition>,
}

impl Permission {
    /// Return `true` when this permission matches the supplied resource/action shape.
    #[must_use]
    pub fn matches(&self, resource: &str, action: &str) -> bool {
        crate::matcher::match_pattern(&self.resource, resource)
            && crate::matcher::match_pattern(&self.action, action)
    }
}

/// RBAC role definition with optional inheritance.
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
pub struct Role {
    /// Role name.
    pub name: String,
    /// Parent roles inherited transitively.
    pub inherits: Vec<String>,
    /// Granted permissions.
    pub permissions: Vec<Permission>,
}

/// Result of a matched policy.
#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
#[non_exhaustive]
pub enum Effect {
    /// Allow the request.
    Allow,
    /// Deny the request.
    #[default]
    Deny,
}

/// Source of an attribute used by a condition.
#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
#[non_exhaustive]
pub enum AttributeSource {
    /// Read from subject attributes.
    #[default]
    Subject,
    /// Read from resource attributes.
    Resource,
    /// Read from request context.
    Context,
}

/// Comparison operator for ABAC conditions.
#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
#[non_exhaustive]
pub enum Operator {
    /// Exact equality.
    #[default]
    Equals,
    /// Exact inequality.
    NotEquals,
    /// Membership in a value list.
    OneOf,
}

/// ABAC condition evaluated against request attributes.
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
pub struct Condition {
    /// Attribute source.
    pub source: AttributeSource,
    /// Attribute key to read.
    pub key: String,
    /// Comparison operator.
    pub operator: Operator,
    /// Literal values to compare against.
    pub values: Vec<Value>,
    /// Optional attribute source for cross-field comparisons.
    pub compare_source: Option<AttributeSource>,
    /// Optional attribute key for cross-field comparisons.
    pub compare_key: Option<String>,
}

/// ABAC policy rule.
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
pub struct Policy {
    /// Policy name.
    pub name: String,
    /// Policy effect.
    pub effect: Effect,
    /// Action patterns.
    pub actions: Vec<String>,
    /// Resource patterns.
    pub resources: Vec<String>,
    /// Attribute conditions.
    pub conditions: Vec<Condition>,
}

/// Final authorization decision.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Decision {
    /// Whether the request is allowed.
    pub allowed: bool,
    /// Human-readable reason.
    pub reason: String,
}
