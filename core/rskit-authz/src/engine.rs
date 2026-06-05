//! Canonical RBAC + ABAC authorization engine.

use std::collections::{HashMap, HashSet};

use rskit_errors::{AppError, AppResult};
use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::checker::Checker;
use crate::matcher::match_any;

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

    fn matches_request(&self, request: &Request) -> bool {
        self.matches(&request.resource.resource_type, &request.action)
            && self
                .conditions
                .iter()
                .all(|condition| condition.matches(request))
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

/// Canonical RBAC + ABAC engine with deny-override and default-deny semantics.
#[derive(Debug, Clone)]
pub struct Engine {
    roles: HashMap<String, Role>,
    policies: Vec<Policy>,
}

impl Engine {
    /// Construct an engine from roles and policies.
    pub fn new(roles: Vec<Role>, policies: Vec<Policy>) -> AppResult<Self> {
        let mut indexed_roles = HashMap::with_capacity(roles.len());
        for role in roles {
            if role.name.is_empty() {
                return Err(AppError::invalid_input(
                    "role.name",
                    "role name is required",
                ));
            }
            if indexed_roles.insert(role.name.clone(), role).is_some() {
                return Err(AppError::invalid_input("role.name", "duplicate role"));
            }
        }
        Ok(Self {
            roles: indexed_roles,
            policies,
        })
    }

    /// Evaluate a request.
    ///
    /// Deny policies are evaluated before role grants and allow policies.
    /// Object-level constraints that should limit a role should be attached to
    /// the role [`Permission`]; constraints that must override any broad grant
    /// should be modeled as deny policies.
    #[must_use]
    pub fn authorize(&self, request: &Request) -> Decision {
        // 1. Deny policies always take precedence.
        for policy in &self.policies {
            if policy.effect == Effect::Deny && policy.matches(request) {
                return Decision {
                    allowed: false,
                    reason: String::from("denied by policy"),
                };
            }
        }

        // 2. RBAC role grants are checked before ABAC allow policies (cross-kit canonical order).
        if self.role_allows(request, &request.subject.roles, &mut HashSet::new()) {
            return Decision {
                allowed: true,
                reason: String::from("allowed by role grant"),
            };
        }

        // 3. ABAC allow policies.
        for policy in &self.policies {
            if policy.effect == Effect::Allow && policy.matches(request) {
                return Decision {
                    allowed: true,
                    reason: String::from("allowed by policy"),
                };
            }
        }

        Decision {
            allowed: false,
            reason: String::from("default deny"),
        }
    }

    fn role_allows(
        &self,
        request: &Request,
        role_names: &[String],
        visited: &mut HashSet<String>,
    ) -> bool {
        for role_name in role_names {
            if !visited.insert(role_name.clone()) {
                continue;
            }
            let Some(role) = self.roles.get(role_name) else {
                continue;
            };
            if role
                .permissions
                .iter()
                .any(|permission| permission.matches_request(request))
            {
                return true;
            }
            if self.role_allows(request, &role.inherits, visited) {
                return true;
            }
        }
        false
    }
}

impl Checker for Engine {
    fn authorize(&self, request: &Request) -> Decision {
        Self::authorize(self, request)
    }
}

impl Policy {
    fn matches(&self, request: &Request) -> bool {
        if !match_dimension(&self.resources, &request.resource.resource_type)
            || !match_dimension(&self.actions, &request.action)
        {
            return false;
        }
        self.conditions
            .iter()
            .all(|condition| condition.matches(request))
    }
}

impl Condition {
    fn matches(&self, request: &Request) -> bool {
        let Some(actual) = attribute_value(request, self.source, &self.key) else {
            return false;
        };

        if let (Some(compare_source), Some(compare_key)) =
            (self.compare_source, self.compare_key.as_deref())
        {
            let Some(other) = attribute_value(request, compare_source, compare_key) else {
                return false;
            };
            // Cross-field comparison: exactly one dynamic value, no Vec allocation.
            match self.operator {
                Operator::Equals | Operator::OneOf => actual == other,
                Operator::NotEquals => actual != other,
            }
        } else {
            // Literal-value comparison: compare by reference, no clone.
            match self.operator {
                Operator::Equals => self.values.len() == 1 && actual == self.values[0],
                Operator::NotEquals => self.values.len() == 1 && actual != self.values[0],
                Operator::OneOf => self.values.iter().any(|v| v == &actual),
            }
        }
    }
}

fn match_dimension(patterns: &[String], value: &str) -> bool {
    !patterns.is_empty() && match_any(patterns, value)
}

fn attribute_value(request: &Request, source: AttributeSource, key: &str) -> Option<Value> {
    match source {
        AttributeSource::Subject => {
            if key == "id" {
                return (!request.subject.id.is_empty())
                    .then(|| Value::String(request.subject.id.clone()));
            }
            request.subject.attributes.get(key).cloned()
        }
        AttributeSource::Resource => match key {
            "id" => (!request.resource.id.is_empty())
                .then(|| Value::String(request.resource.id.clone())),
            "type" => (!request.resource.resource_type.is_empty())
                .then(|| Value::String(request.resource.resource_type.clone())),
            _ => request.resource.attributes.get(key).cloned(),
        },
        AttributeSource::Context => request.context.get(key).cloned(),
    }
}
