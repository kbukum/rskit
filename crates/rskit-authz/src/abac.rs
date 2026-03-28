use std::collections::HashMap;

use async_trait::async_trait;

use rskit_errors::{AppError, AppResult};

use crate::checker::Checker;
use crate::rbac::Effect;

/// A single attribute-based rule.
///
/// Implementations inspect the caller's claims and the requested action/resource
/// to produce an access decision. Return `None` to abstain and let the next
/// rule decide.
pub trait AbacRule: Send + Sync {
    /// Evaluate the rule against the given claims.
    fn evaluate(
        &self,
        claims: &HashMap<String, serde_json::Value>,
        action: &str,
        resource: &str,
    ) -> Option<Effect>;
}

/// Attribute-Based Access Control checker.
///
/// Rules are evaluated in order; the first rule that returns a definitive
/// [`Effect`] wins. If no rule matches, access is denied by default.
pub struct AbacChecker {
    rules: Vec<Box<dyn AbacRule>>,
}

impl AbacChecker {
    /// Create an empty checker.
    pub fn new() -> Self {
        Self { rules: Vec::new() }
    }

    /// Append a rule to the evaluation chain.
    pub fn add_rule(&mut self, rule: Box<dyn AbacRule>) {
        self.rules.push(rule);
    }
}

impl Default for AbacChecker {
    fn default() -> Self {
        Self::new()
    }
}

/// The [`Checker`] implementation treats the `subject` parameter as a JSON
/// object of claims. If the subject cannot be parsed, the check is denied.
#[async_trait]
impl Checker for AbacChecker {
    async fn check(&self, subject: &str, action: &str, resource: &str) -> AppResult<()> {
        let claims: HashMap<String, serde_json::Value> =
            serde_json::from_str(subject).unwrap_or_default();

        for rule in &self.rules {
            match rule.evaluate(&claims, action, resource) {
                Some(Effect::Allow) => return Ok(()),
                Some(Effect::Deny) => {
                    return Err(AppError::forbidden(format!(
                        "ABAC rule denied {action} on {resource}"
                    )));
                }
                None => continue,
            }
        }

        // Default deny
        Err(AppError::forbidden(format!(
            "no ABAC rule grants {action} on {resource}"
        )))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    struct AllowRole {
        role: String,
    }

    impl AbacRule for AllowRole {
        fn evaluate(
            &self,
            claims: &HashMap<String, serde_json::Value>,
            _action: &str,
            _resource: &str,
        ) -> Option<Effect> {
            claims.get("role").and_then(|v| v.as_str()).and_then(|r| {
                if r == self.role {
                    Some(Effect::Allow)
                } else {
                    None
                }
            })
        }
    }

    #[tokio::test]
    async fn abac_allow_by_role() {
        let mut checker = AbacChecker::new();
        checker.add_rule(Box::new(AllowRole {
            role: "admin".into(),
        }));

        let claims = r#"{"role":"admin"}"#;
        assert!(checker.check(claims, "delete", "anything").await.is_ok());
    }

    #[tokio::test]
    async fn abac_default_deny() {
        let checker = AbacChecker::new();
        let claims = r#"{"role":"guest"}"#;
        assert!(checker.check(claims, "read", "secret").await.is_err());
    }
}
