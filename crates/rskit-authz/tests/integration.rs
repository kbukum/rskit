use std::collections::HashMap;

use rskit_authz::{AbacChecker, AbacRule, Checker, Effect, Policy, RbacChecker};

// ═══════════════════════════════════════════════════════════════════════════════
// RBAC
// ═══════════════════════════════════════════════════════════════════════════════

fn allow_policy(subject: &str, action: &str, resource: &str) -> Policy {
    Policy {
        subject: subject.into(),
        action: action.into(),
        resource: resource.into(),
        effect: Effect::Allow,
    }
}

fn deny_policy(subject: &str, action: &str, resource: &str) -> Policy {
    Policy {
        subject: subject.into(),
        action: action.into(),
        resource: resource.into(),
        effect: Effect::Deny,
    }
}

#[tokio::test]
async fn rbac_allows_matching_policy() {
    let checker = RbacChecker::new(vec![allow_policy("admin", "read", "document")]);
    let result = checker.check("admin", "read", "document").await;
    assert!(result.is_ok());
}

#[tokio::test]
async fn rbac_denies_when_no_policy_matches() {
    let checker = RbacChecker::new(vec![allow_policy("admin", "read", "document")]);
    let result = checker.check("guest", "read", "document").await;
    assert!(result.is_err());
}

#[tokio::test]
async fn rbac_explicit_deny_overrides_allow() {
    let checker = RbacChecker::new(vec![
        allow_policy("admin", "delete", "document"),
        deny_policy("admin", "delete", "document"),
    ]);
    let result = checker.check("admin", "delete", "document").await;
    assert!(result.is_err());
}

#[tokio::test]
async fn rbac_add_policy_grants_access() {
    let mut checker = RbacChecker::new(vec![]);
    let result = checker.check("editor", "write", "page").await;
    assert!(result.is_err());

    checker.add_policy(allow_policy("editor", "write", "page"));
    let result = checker.check("editor", "write", "page").await;
    assert!(result.is_ok());
}

#[tokio::test]
async fn rbac_remove_policy_revokes_access() {
    let mut checker = RbacChecker::new(vec![allow_policy("editor", "write", "page")]);
    assert!(checker.check("editor", "write", "page").await.is_ok());

    checker.remove_policy("editor", "write", "page");
    assert!(checker.check("editor", "write", "page").await.is_err());
}

// ── wildcard matching ─────────────────────────────────────────────────────────

#[tokio::test]
async fn rbac_wildcard_subject_matches_any_user() {
    let checker = RbacChecker::new(vec![allow_policy("*", "read", "public")]);
    assert!(checker.check("anyone", "read", "public").await.is_ok());
    assert!(checker.check("anonymous", "read", "public").await.is_ok());
}

#[tokio::test]
async fn rbac_wildcard_action_matches_any_action() {
    let checker = RbacChecker::new(vec![allow_policy("superadmin", "*", "everything")]);
    assert!(
        checker
            .check("superadmin", "read", "everything")
            .await
            .is_ok()
    );
    assert!(
        checker
            .check("superadmin", "delete", "everything")
            .await
            .is_ok()
    );
}

#[tokio::test]
async fn rbac_wildcard_resource_matches_any_resource() {
    let checker = RbacChecker::new(vec![allow_policy("admin", "read", "*")]);
    assert!(checker.check("admin", "read", "users").await.is_ok());
    assert!(checker.check("admin", "read", "logs").await.is_ok());
}

// ═══════════════════════════════════════════════════════════════════════════════
// ABAC
// ═══════════════════════════════════════════════════════════════════════════════

struct DepartmentRule {
    allowed_dept: String,
    target_action: String,
}

impl AbacRule for DepartmentRule {
    fn evaluate(
        &self,
        claims: &HashMap<String, serde_json::Value>,
        action: &str,
        _resource: &str,
    ) -> Option<Effect> {
        if action != self.target_action {
            return None;
        }
        match claims.get("department").and_then(|v| v.as_str()) {
            Some(dept) if dept == self.allowed_dept => Some(Effect::Allow),
            Some(_) => Some(Effect::Deny),
            None => None,
        }
    }
}

fn subject_json(claims: &[(&str, &str)]) -> String {
    let map: HashMap<&str, &str> = claims.iter().copied().collect();
    serde_json::to_string(&map).unwrap()
}

#[tokio::test]
async fn abac_allows_when_rule_matches() {
    let mut checker = AbacChecker::new();
    checker.add_rule(Box::new(DepartmentRule {
        allowed_dept: "engineering".into(),
        target_action: "deploy".into(),
    }));

    let subject = subject_json(&[("department", "engineering")]);
    let result = checker.check(&subject, "deploy", "prod").await;
    assert!(result.is_ok());
}

#[tokio::test]
async fn abac_denies_when_rule_rejects() {
    let mut checker = AbacChecker::new();
    checker.add_rule(Box::new(DepartmentRule {
        allowed_dept: "engineering".into(),
        target_action: "deploy".into(),
    }));

    let subject = subject_json(&[("department", "marketing")]);
    let result = checker.check(&subject, "deploy", "prod").await;
    assert!(result.is_err());
}

#[tokio::test]
async fn abac_denies_by_default_when_no_rules() {
    let checker = AbacChecker::new();
    let subject = subject_json(&[("department", "engineering")]);
    let result = checker.check(&subject, "deploy", "prod").await;
    assert!(result.is_err());
}

#[tokio::test]
async fn abac_denies_when_rule_abstains() {
    let mut checker = AbacChecker::new();
    checker.add_rule(Box::new(DepartmentRule {
        allowed_dept: "engineering".into(),
        target_action: "deploy".into(),
    }));

    // Action doesn't match the rule's target_action, so the rule abstains → deny
    let subject = subject_json(&[("department", "engineering")]);
    let result = checker.check(&subject, "read", "docs").await;
    assert!(result.is_err());
}

#[tokio::test]
async fn abac_first_decisive_rule_wins() {
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

    // Allow rule is first → should allow even though deny follows
    let mut checker = AbacChecker::new();
    checker.add_rule(Box::new(AlwaysAllow));
    checker.add_rule(Box::new(AlwaysDeny));

    let subject = subject_json(&[]);
    assert!(checker.check(&subject, "any", "thing").await.is_ok());
}
