//! Behavioral tests for authorization policy matching, roles, and ABAC checks.

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
fn empty_role_names_are_rejected() {
    let result = Engine::new(
        vec![Role {
            name: String::new(),
            inherits: vec![],
            permissions: vec![],
        }],
        vec![],
    );

    assert!(result.is_err());
}

#[test]
fn permission_matching_supports_wildcards() {
    let permission = Permission {
        resource: String::from("article"),
        action: String::from("*"),
        conditions: vec![],
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

#[test]
fn abac_conditions_fail_closed_for_missing_attributes() {
    let engine = Engine::new(
        vec![Role {
            name: String::from("owner"),
            inherits: vec![],
            permissions: vec![Permission {
                resource: String::from("document"),
                action: String::from("read"),
                conditions: vec![Condition {
                    source: AttributeSource::Resource,
                    key: String::from("owner_id"),
                    operator: Operator::Equals,
                    values: vec![],
                    compare_source: Some(AttributeSource::Subject),
                    compare_key: Some(String::from("id")),
                }],
            }],
        }],
        vec![Policy {
            name: String::from("context-required"),
            effect: Effect::Allow,
            actions: vec![String::from("approve")],
            resources: vec![String::from("document")],
            conditions: vec![Condition {
                source: AttributeSource::Context,
                key: String::from("ticket"),
                operator: Operator::Equals,
                values: vec![json!("approved")],
                compare_source: None,
                compare_key: None,
            }],
        }],
    )
    .unwrap();

    let mut missing_resource_owner = empty_request();
    missing_resource_owner.subject.roles = vec![String::from("owner")];
    missing_resource_owner.resource.resource_type = String::from("document");
    assert!(!engine.check(&missing_resource_owner));

    let mut missing_context = missing_resource_owner.clone();
    missing_context.subject.roles.clear();
    missing_context.action = String::from("approve");
    assert!(!engine.check(&missing_context));
}

#[test]
fn resource_id_and_type_can_be_used_as_authorization_attributes() {
    let engine = Engine::new(
        vec![],
        vec![Policy {
            name: String::from("specific-document"),
            effect: Effect::Allow,
            actions: vec![String::from("read")],
            resources: vec![String::from("document")],
            conditions: vec![
                Condition {
                    source: AttributeSource::Resource,
                    key: String::from("id"),
                    operator: Operator::Equals,
                    values: vec![json!("doc-1")],
                    compare_source: None,
                    compare_key: None,
                },
                Condition {
                    source: AttributeSource::Resource,
                    key: String::from("type"),
                    operator: Operator::Equals,
                    values: vec![json!("document")],
                    compare_source: None,
                    compare_key: None,
                },
            ],
        }],
    )
    .unwrap();

    let mut request = empty_request();
    request.subject.roles.clear();
    request.resource.resource_type = String::from("document");
    request.resource.id = String::from("doc-1");
    assert!(engine.check(&request));

    request.resource.id.clear();
    assert!(!engine.check(&request));
}

#[test]
fn cyclic_role_inheritance_is_visited_once_and_fails_closed_on_miss() {
    let engine = Engine::new(
        vec![
            Role {
                name: String::from("a"),
                inherits: vec![String::from("b")],
                permissions: vec![],
            },
            Role {
                name: String::from("b"),
                inherits: vec![String::from("a")],
                permissions: vec![],
            },
        ],
        vec![],
    )
    .unwrap();

    let mut request = empty_request();
    request.subject.roles = vec![String::from("a")];

    let decision = engine.authorize(&request);
    assert!(!decision.allowed);
    assert_eq!(decision.reason, "default deny");
}

#[test]
fn cross_attribute_comparison_fails_closed_when_compare_value_is_missing() {
    let engine = Engine::new(
        vec![],
        vec![Policy {
            name: String::from("tenant-match"),
            effect: Effect::Allow,
            actions: vec![String::from("read")],
            resources: vec![String::from("invoice")],
            conditions: vec![Condition {
                source: AttributeSource::Subject,
                key: String::from("tenant_id"),
                operator: Operator::Equals,
                values: vec![],
                compare_source: Some(AttributeSource::Resource),
                compare_key: Some(String::from("tenant_id")),
            }],
        }],
    )
    .unwrap();

    let mut request = empty_request();
    request.subject.roles.clear();
    request.resource.resource_type = String::from("invoice");
    request
        .subject
        .attributes
        .insert(String::from("tenant_id"), json!("acme"));

    assert!(!engine.check(&request));
}
