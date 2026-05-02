use std::collections::HashMap;

use rskit_authz::{
    AttributeSource, Checker, Condition, Effect, Engine, Operator, Permission, Policy, Request,
    Resource, Role, Subject, match_pattern,
};
use serde_json::json;

fn empty_request() -> Request {
    Request {
        subject: Subject {
            id: String::from("user-1"),
            roles: vec![String::from("viewer")],
            attributes: HashMap::new(),
        },
        resource: Resource {
            resource_type: String::from("article"),
            id: String::from("article-1"),
            attributes: HashMap::new(),
        },
        action: String::from("read"),
        context: HashMap::new(),
    }
}

#[test]
fn matcher_never_treats_request_values_as_patterns() {
    assert!(!match_pattern("article:read", "*:*"));
}

#[test]
fn duplicate_roles_are_rejected() {
    let result = Engine::new(
        vec![
            Role {
                name: String::from("viewer"),
                inherits: vec![],
                permissions: vec![],
            },
            Role {
                name: String::from("viewer"),
                inherits: vec![],
                permissions: vec![],
            },
        ],
        vec![],
    );
    assert!(result.is_err());
}

#[test]
fn permission_matching_supports_wildcards() {
    let permission = Permission {
        resource: String::from("article"),
        action: String::from("*"),
    };
    assert!(permission.matches("article", "read"));
    assert!(!permission.matches("invoice", "read"));
}

#[test]
fn default_deny_applies_when_nothing_matches() {
    let engine = Engine::new(vec![], vec![]).unwrap();
    let decision = engine.authorize(&empty_request());
    assert!(!decision.allowed);
    assert_eq!(decision.reason, "default deny");
}

#[test]
fn abac_condition_supports_one_of_and_context_attributes() {
    let engine = Engine::new(
        vec![],
        vec![Policy {
            name: String::from("regional-support"),
            effect: Effect::Allow,
            actions: vec![String::from("read")],
            resources: vec![String::from("ticket")],
            conditions: vec![
                Condition {
                    source: AttributeSource::Context,
                    key: String::from("region"),
                    operator: Operator::OneOf,
                    values: vec![json!("eu"), json!("us")],
                    compare_source: None,
                    compare_key: None,
                },
                Condition {
                    source: AttributeSource::Resource,
                    key: String::from("classification"),
                    operator: Operator::NotEquals,
                    values: vec![json!("secret")],
                    compare_source: None,
                    compare_key: None,
                },
            ],
        }],
    )
    .unwrap();

    let mut request = empty_request();
    request.resource.resource_type = String::from("ticket");
    request
        .resource
        .attributes
        .insert(String::from("classification"), json!("internal"));
    request.context.insert(String::from("region"), json!("eu"));
    assert!(engine.check(&request));

    request
        .resource
        .attributes
        .insert(String::from("classification"), json!("secret"));
    assert!(!engine.check(&request));
}
