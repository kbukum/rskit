use async_trait::async_trait;
use serde::{Deserialize, Serialize};

use rskit_errors::{AppError, AppResult};

use crate::checker::Checker;

/// The effect of a matched policy rule.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum Effect {
    /// Explicitly allow the action.
    Allow,
    /// Explicitly deny the action.
    Deny,
}

/// A single RBAC policy entry.
///
/// Subjects, actions and resources support a simple wildcard: `"*"` matches
/// any value.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Policy {
    /// Principal (user, role, group).
    pub subject: String,
    /// Operation being performed.
    pub action: String,
    /// Target resource path or identifier.
    pub resource: String,
    /// Whether this policy allows or denies the action.
    pub effect: Effect,
}

/// Role-Based Access Control checker.
///
/// Evaluation order:
/// 1. All `Deny` policies are checked first — an explicit deny always wins.
/// 2. If no deny matches, any matching `Allow` grants access.
/// 3. If nothing matches, access is denied by default.
pub struct RbacChecker {
    policies: Vec<Policy>,
}

impl RbacChecker {
    /// Create a new checker pre-loaded with the given policies.
    pub fn new(policies: Vec<Policy>) -> Self {
        Self { policies }
    }

    /// Append a policy.
    pub fn add_policy(&mut self, policy: Policy) {
        self.policies.push(policy);
    }

    /// Remove the first policy matching the given (subject, action, resource) triple.
    pub fn remove_policy(&mut self, subject: &str, action: &str, resource: &str) {
        self.policies
            .retain(|p| !(p.subject == subject && p.action == action && p.resource == resource));
    }
}

/// Simple wildcard match — `"*"` matches everything, otherwise exact equality.
fn matches(pattern: &str, value: &str) -> bool {
    pattern == "*" || pattern == value
}

#[async_trait]
impl Checker for RbacChecker {
    async fn check(&self, subject: &str, action: &str, resource: &str) -> AppResult<()> {
        // Explicit deny first
        for p in &self.policies {
            if p.effect == Effect::Deny
                && matches(&p.subject, subject)
                && matches(&p.action, action)
                && matches(&p.resource, resource)
            {
                return Err(AppError::forbidden(format!(
                    "denied by policy for {subject}/{action}/{resource}"
                )));
            }
        }

        // Then look for an allow
        for p in &self.policies {
            if p.effect == Effect::Allow
                && matches(&p.subject, subject)
                && matches(&p.action, action)
                && matches(&p.resource, resource)
            {
                return Ok(());
            }
        }

        // Default deny
        Err(AppError::forbidden(format!(
            "no policy grants {subject}/{action}/{resource}"
        )))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn allow(subject: &str, action: &str, resource: &str) -> Policy {
        Policy {
            subject: subject.into(),
            action: action.into(),
            resource: resource.into(),
            effect: Effect::Allow,
        }
    }

    fn deny(subject: &str, action: &str, resource: &str) -> Policy {
        Policy {
            subject: subject.into(),
            action: action.into(),
            resource: resource.into(),
            effect: Effect::Deny,
        }
    }

    #[tokio::test]
    async fn allow_exact_match() {
        let checker = RbacChecker::new(vec![allow("alice", "read", "doc:1")]);
        assert!(checker.check("alice", "read", "doc:1").await.is_ok());
    }

    #[tokio::test]
    async fn deny_overrides_allow() {
        let checker = RbacChecker::new(vec![
            allow("alice", "*", "doc:1"),
            deny("alice", "delete", "doc:1"),
        ]);
        assert!(checker.check("alice", "delete", "doc:1").await.is_err());
    }

    #[tokio::test]
    async fn default_deny() {
        let checker = RbacChecker::new(vec![]);
        assert!(checker.check("alice", "read", "doc:1").await.is_err());
    }

    #[tokio::test]
    async fn wildcard_subject() {
        let checker = RbacChecker::new(vec![allow("*", "read", "public")]);
        assert!(checker.check("anyone", "read", "public").await.is_ok());
    }

    #[tokio::test]
    async fn remove_policy() {
        let mut checker = RbacChecker::new(vec![allow("alice", "read", "doc:1")]);
        checker.remove_policy("alice", "read", "doc:1");
        assert!(checker.check("alice", "read", "doc:1").await.is_err());
    }
}
