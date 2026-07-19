//! Runtime evaluation: deny-override, default-deny RBAC role grants and ABAC policy matching.

use std::collections::{HashMap, HashSet};

use rskit_errors::{AppError, AppResult};
use serde_json::Value;

use super::model::{
    AttributeSource, Condition, Decision, Effect, Operator, Permission, Policy, Request, Role,
};
use crate::checker::Checker;
use crate::matcher::match_any;

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
    /// Object-level constraints that should limit a role should be attached to the role [`Permission`];
    /// constraints that must override any broad grant should be modeled as deny policies.
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

impl Permission {
    fn matches_request(&self, request: &Request) -> bool {
        self.matches(&request.resource.resource_type, &request.action)
            && self
                .conditions
                .iter()
                .all(|condition| condition.matches(request))
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
