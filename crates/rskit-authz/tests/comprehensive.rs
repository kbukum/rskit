use std::collections::HashMap;

use async_trait::async_trait;

use rskit_authz::{AbacChecker, AbacRule, Checker, Effect, Policy, RbacChecker};

// ═══════════════════════════════════════════════════════════════════════════════
// Helpers
// ═══════════════════════════════════════════════════════════════════════════════

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

fn subject_json(claims: &[(&str, serde_json::Value)]) -> String {
    let map: HashMap<&str, &serde_json::Value> = claims.iter().map(|(k, v)| (*k, v)).collect();
    serde_json::to_string(&map).unwrap()
}

// ═══════════════════════════════════════════════════════════════════════════════
// Checker trait contract tests
// ═══════════════════════════════════════════════════════════════════════════════

struct AlwaysAllowChecker;

#[async_trait]
impl Checker for AlwaysAllowChecker {
    async fn check(&self, _subject: &str, _action: &str, _resource: &str) -> rskit_errors::AppResult<()> {
        Ok(())
    }
}

struct AlwaysDenyChecker;

#[async_trait]
impl Checker for AlwaysDenyChecker {
    async fn check(&self, _subject: &str, _action: &str, _resource: &str) -> rskit_errors::AppResult<()> {
        Err(rskit_errors::AppError::forbidden("always denied"))
    }
}

#[tokio::test]
async fn checker_trait_always_allow() {
    let checker = AlwaysAllowChecker;
    assert!(checker.check("anyone", "any", "thing").await.is_ok());
}

#[tokio::test]
async fn checker_trait_always_deny() {
    let checker = AlwaysDenyChecker;
    assert!(checker.check("anyone", "any", "thing").await.is_err());
}

#[tokio::test]
async fn checker_trait_custom_logic() {
    struct AdminOnlyChecker;

    #[async_trait]
    impl Checker for AdminOnlyChecker {
        async fn check(&self, subject: &str, _action: &str, _resource: &str) -> rskit_errors::AppResult<()> {
            if subject == "admin" {
                Ok(())
            } else {
                Err(rskit_errors::AppError::forbidden("admin only"))
            }
        }
    }

    let checker = AdminOnlyChecker;
    assert!(checker.check("admin", "read", "anything").await.is_ok());
    assert!(checker.check("guest", "read", "anything").await.is_err());
}

// ═══════════════════════════════════════════════════════════════════════════════
// RBAC — security and edge cases
// ═══════════════════════════════════════════════════════════════════════════════

#[tokio::test]
async fn rbac_default_deny_no_policies() {
    let checker = RbacChecker::new(vec![]);
    assert!(checker.check("alice", "read", "doc").await.is_err());
}

#[tokio::test]
async fn rbac_default_deny_unknown_subject() {
    let checker = RbacChecker::new(vec![allow("alice", "read", "doc")]);
    assert!(checker.check("bob", "read", "doc").await.is_err());
}

#[tokio::test]
async fn rbac_default_deny_wrong_action() {
    let checker = RbacChecker::new(vec![allow("alice", "read", "doc")]);
    assert!(checker.check("alice", "write", "doc").await.is_err());
}

#[tokio::test]
async fn rbac_default_deny_wrong_resource() {
    let checker = RbacChecker::new(vec![allow("alice", "read", "doc")]);
    assert!(checker.check("alice", "read", "secret").await.is_err());
}

#[tokio::test]
async fn rbac_deny_always_overrides_allow_regardless_of_order() {
    // Deny after allow
    let checker = RbacChecker::new(vec![
        allow("alice", "delete", "doc"),
        deny("alice", "delete", "doc"),
    ]);
    assert!(checker.check("alice", "delete", "doc").await.is_err());

    // Deny before allow
    let checker = RbacChecker::new(vec![
        deny("alice", "delete", "doc"),
        allow("alice", "delete", "doc"),
    ]);
    assert!(checker.check("alice", "delete", "doc").await.is_err());
}

#[tokio::test]
async fn rbac_deny_specific_action_allow_wildcard() {
    let checker = RbacChecker::new(vec![
        allow("alice", "*", "doc"),
        deny("alice", "delete", "doc"),
    ]);
    assert!(checker.check("alice", "read", "doc").await.is_ok());
    assert!(checker.check("alice", "delete", "doc").await.is_err());
}

#[tokio::test]
async fn rbac_wildcard_all_three() {
    let checker = RbacChecker::new(vec![allow("*", "*", "*")]);
    assert!(checker.check("anyone", "anything", "anywhere").await.is_ok());
}

#[tokio::test]
async fn rbac_wildcard_deny_all() {
    let checker = RbacChecker::new(vec![deny("*", "*", "*")]);
    assert!(checker.check("anyone", "anything", "anywhere").await.is_err());
}

#[tokio::test]
async fn rbac_multiple_policies_for_same_subject() {
    let checker = RbacChecker::new(vec![
        allow("alice", "read", "doc"),
        allow("alice", "write", "doc"),
        allow("alice", "read", "config"),
    ]);
    assert!(checker.check("alice", "read", "doc").await.is_ok());
    assert!(checker.check("alice", "write", "doc").await.is_ok());
    assert!(checker.check("alice", "read", "config").await.is_ok());
    assert!(checker.check("alice", "delete", "doc").await.is_err());
}

#[tokio::test]
async fn rbac_case_sensitivity_subject() {
    let checker = RbacChecker::new(vec![allow("alice", "read", "doc")]);
    assert!(checker.check("Alice", "read", "doc").await.is_err());
    assert!(checker.check("ALICE", "read", "doc").await.is_err());
}

#[tokio::test]
async fn rbac_case_sensitivity_action() {
    let checker = RbacChecker::new(vec![allow("alice", "read", "doc")]);
    assert!(checker.check("alice", "Read", "doc").await.is_err());
    assert!(checker.check("alice", "READ", "doc").await.is_err());
}

#[tokio::test]
async fn rbac_case_sensitivity_resource() {
    let checker = RbacChecker::new(vec![allow("alice", "read", "doc")]);
    assert!(checker.check("alice", "read", "Doc").await.is_err());
    assert!(checker.check("alice", "read", "DOC").await.is_err());
}

#[tokio::test]
async fn rbac_empty_strings() {
    let checker = RbacChecker::new(vec![allow("", "", "")]);
    assert!(checker.check("", "", "").await.is_ok());
    assert!(checker.check("alice", "", "").await.is_err());
}

#[tokio::test]
async fn rbac_add_then_remove_policy() {
    let mut checker = RbacChecker::new(vec![]);
    assert!(checker.check("editor", "write", "page").await.is_err());

    checker.add_policy(allow("editor", "write", "page"));
    assert!(checker.check("editor", "write", "page").await.is_ok());

    checker.remove_policy("editor", "write", "page");
    assert!(checker.check("editor", "write", "page").await.is_err());
}

#[tokio::test]
async fn rbac_remove_nonexistent_policy_is_noop() {
    let mut checker = RbacChecker::new(vec![allow("alice", "read", "doc")]);
    checker.remove_policy("bob", "write", "secret");
    // Original policy still works
    assert!(checker.check("alice", "read", "doc").await.is_ok());
}

#[tokio::test]
async fn rbac_large_policy_set() {
    let policies: Vec<Policy> = (0..500)
        .map(|i| allow(&format!("user_{i}"), "read", &format!("resource_{i}")))
        .collect();
    let checker = RbacChecker::new(policies);

    assert!(checker.check("user_0", "read", "resource_0").await.is_ok());
    assert!(checker.check("user_499", "read", "resource_499").await.is_ok());
    assert!(checker.check("user_0", "read", "resource_1").await.is_err());
    assert!(checker.check("nonexistent", "read", "resource_0").await.is_err());
}

#[tokio::test]
async fn rbac_wildcard_does_not_cross_subjects() {
    let checker = RbacChecker::new(vec![
        allow("admin", "*", "*"),
        allow("viewer", "read", "*"),
    ]);
    // viewer must NOT get write access through admin's wildcard
    assert!(checker.check("viewer", "write", "anything").await.is_err());
    assert!(checker.check("viewer", "read", "anything").await.is_ok());
}

// ═══════════════════════════════════════════════════════════════════════════════
// ABAC — extended tests
// ═══════════════════════════════════════════════════════════════════════════════

struct AgeGateRule {
    min_age: u64,
}

impl AbacRule for AgeGateRule {
    fn evaluate(
        &self,
        claims: &HashMap<String, serde_json::Value>,
        _action: &str,
        _resource: &str,
    ) -> Option<Effect> {
        claims.get("age").and_then(|v| v.as_u64()).map(|age| {
            if age >= self.min_age {
                Effect::Allow
            } else {
                Effect::Deny
            }
        })
    }
}

struct DepartmentRule {
    allowed_dept: String,
}

impl AbacRule for DepartmentRule {
    fn evaluate(
        &self,
        claims: &HashMap<String, serde_json::Value>,
        _action: &str,
        _resource: &str,
    ) -> Option<Effect> {
        claims
            .get("department")
            .and_then(|v| v.as_str())
            .map(|dept| {
                if dept == self.allowed_dept {
                    Effect::Allow
                } else {
                    Effect::Deny
                }
            })
    }
}

struct ActionGateRule {
    allowed_action: String,
}

impl AbacRule for ActionGateRule {
    fn evaluate(
        &self,
        _claims: &HashMap<String, serde_json::Value>,
        action: &str,
        _resource: &str,
    ) -> Option<Effect> {
        if action == self.allowed_action {
            Some(Effect::Allow)
        } else {
            None
        }
    }
}

#[tokio::test]
async fn abac_age_gate_allows_adult() {
    let mut checker = AbacChecker::new();
    checker.add_rule(Box::new(AgeGateRule { min_age: 18 }));

    let subject = subject_json(&[("age", serde_json::json!(21))]);
    assert!(checker.check(&subject, "view", "content").await.is_ok());
}

#[tokio::test]
async fn abac_age_gate_denies_minor() {
    let mut checker = AbacChecker::new();
    checker.add_rule(Box::new(AgeGateRule { min_age: 18 }));

    let subject = subject_json(&[("age", serde_json::json!(16))]);
    assert!(checker.check(&subject, "view", "content").await.is_err());
}

#[tokio::test]
async fn abac_age_gate_abstains_when_no_age() {
    let mut checker = AbacChecker::new();
    checker.add_rule(Box::new(AgeGateRule { min_age: 18 }));

    // No age claim → rule abstains → default deny
    let subject = subject_json(&[("name", serde_json::json!("alice"))]);
    assert!(checker.check(&subject, "view", "content").await.is_err());
}

#[tokio::test]
async fn abac_first_decisive_rule_allow_then_deny() {
    struct AlwaysAllow;
    impl AbacRule for AlwaysAllow {
        fn evaluate(
            &self,
            _claims: &HashMap<String, serde_json::Value>,
            _action: &str,
            _resource: &str,
        ) -> Option<Effect> {
            Some(Effect::Allow)
        }
    }

    struct AlwaysDeny;
    impl AbacRule for AlwaysDeny {
        fn evaluate(
            &self,
            _claims: &HashMap<String, serde_json::Value>,
            _action: &str,
            _resource: &str,
        ) -> Option<Effect> {
            Some(Effect::Deny)
        }
    }

    // Allow first → should allow
    let mut checker = AbacChecker::new();
    checker.add_rule(Box::new(AlwaysAllow));
    checker.add_rule(Box::new(AlwaysDeny));
    let subject = subject_json(&[]);
    assert!(checker.check(&subject, "any", "thing").await.is_ok());
}

#[tokio::test]
async fn abac_first_decisive_rule_deny_then_allow() {
    struct AlwaysAllow;
    impl AbacRule for AlwaysAllow {
        fn evaluate(
            &self,
            _claims: &HashMap<String, serde_json::Value>,
            _action: &str,
            _resource: &str,
        ) -> Option<Effect> {
            Some(Effect::Allow)
        }
    }

    struct AlwaysDeny;
    impl AbacRule for AlwaysDeny {
        fn evaluate(
            &self,
            _claims: &HashMap<String, serde_json::Value>,
            _action: &str,
            _resource: &str,
        ) -> Option<Effect> {
            Some(Effect::Deny)
        }
    }

    // Deny first → should deny
    let mut checker = AbacChecker::new();
    checker.add_rule(Box::new(AlwaysDeny));
    checker.add_rule(Box::new(AlwaysAllow));
    let subject = subject_json(&[]);
    assert!(checker.check(&subject, "any", "thing").await.is_err());
}

#[tokio::test]
async fn abac_abstaining_rule_falls_through_to_next() {
    let mut checker = AbacChecker::new();
    // First rule only matches "deploy" action
    checker.add_rule(Box::new(ActionGateRule {
        allowed_action: "deploy".into(),
    }));
    // Second rule checks department
    checker.add_rule(Box::new(DepartmentRule {
        allowed_dept: "engineering".into(),
    }));

    // "read" action → ActionGateRule abstains → DepartmentRule decides
    let subject = subject_json(&[("department", serde_json::json!("engineering"))]);
    assert!(checker.check(&subject, "read", "docs").await.is_ok());

    // "deploy" → ActionGateRule allows immediately
    let subject = subject_json(&[("department", serde_json::json!("marketing"))]);
    assert!(checker.check(&subject, "deploy", "prod").await.is_ok());
}

#[tokio::test]
async fn abac_invalid_json_subject_defaults_to_empty_claims() {
    let mut checker = AbacChecker::new();
    checker.add_rule(Box::new(AgeGateRule { min_age: 18 }));

    // Invalid JSON → parsed as empty map → rule abstains → default deny
    assert!(checker.check("not-json", "view", "content").await.is_err());
}

#[tokio::test]
async fn abac_empty_json_object() {
    let mut checker = AbacChecker::new();
    checker.add_rule(Box::new(AgeGateRule { min_age: 18 }));

    assert!(checker.check("{}", "view", "content").await.is_err());
}

#[tokio::test]
async fn abac_default_deny_no_rules() {
    let checker = AbacChecker::new();
    let subject = subject_json(&[("role", serde_json::json!("admin"))]);
    assert!(checker.check(&subject, "any", "thing").await.is_err());
}

// ═══════════════════════════════════════════════════════════════════════════════
// Security-focused tests
// ═══════════════════════════════════════════════════════════════════════════════

#[tokio::test]
async fn security_rbac_wildcard_in_requested_value_no_auto_grant() {
    // If a caller sends "*" as subject/action/resource, it should NOT match
    // non-wildcard policies (wildcards only work in patterns, not values)
    let checker = RbacChecker::new(vec![allow("alice", "read", "doc")]);
    // "*" as subject won't match "alice" policy — the match is pattern→value, not value→pattern
    assert!(checker.check("*", "read", "doc").await.is_err());
}

#[tokio::test]
async fn security_rbac_deny_escalation_via_wildcard() {
    // A deny("*", "*", "*") must block everything even if allows exist
    let checker = RbacChecker::new(vec![
        allow("admin", "*", "*"),
        deny("*", "*", "*"),
    ]);
    assert!(checker.check("admin", "read", "anything").await.is_err());
}

#[tokio::test]
async fn security_abac_privilege_escalation_via_claims() {
    // Attacker tries to pass claims that grant access
    struct RoleChecker;
    impl AbacRule for RoleChecker {
        fn evaluate(
            &self,
            claims: &HashMap<String, serde_json::Value>,
            _action: &str,
            _resource: &str,
        ) -> Option<Effect> {
            claims
                .get("role")
                .and_then(|v| v.as_str())
                .map(|role| {
                    if role == "admin" {
                        Effect::Allow
                    } else {
                        Effect::Deny
                    }
                })
        }
    }

    let mut checker = AbacChecker::new();
    checker.add_rule(Box::new(RoleChecker));

    // Legitimate admin
    let admin = r#"{"role":"admin"}"#;
    assert!(checker.check(admin, "delete", "anything").await.is_ok());

    // Non-admin
    let guest = r#"{"role":"guest"}"#;
    assert!(checker.check(guest, "delete", "anything").await.is_err());

    // No role claim at all → abstains → default deny
    assert!(checker.check("{}", "delete", "anything").await.is_err());
}

#[tokio::test]
async fn security_empty_subject_empty_action_empty_resource() {
    // Empty strings should not accidentally grant access
    let checker = RbacChecker::new(vec![allow("admin", "read", "doc")]);
    assert!(checker.check("", "", "").await.is_err());
    assert!(checker.check("", "read", "doc").await.is_err());
    assert!(checker.check("admin", "", "").await.is_err());
}
