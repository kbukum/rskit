use std::collections::HashMap;

use rskit_authz::{
    AttributeSource, Checker, Condition, Effect, Engine, Operator, Permission, Policy, Request,
    Resource, Role, Subject,
};
use serde_json::json;

fn request(role: &str, action: &str, resource_type: &str) -> Request {
    Request {
        subject: Subject {
            id: String::from("user-1"),
            roles: vec![role.to_string()],
            attributes: HashMap::new(),
        },
        resource: Resource {
            resource_type: resource_type.to_string(),
            id: String::from("resource-1"),
            attributes: HashMap::new(),
        },
        action: action.to_string(),
        context: HashMap::new(),
    }
}

#[test]
fn engine_supports_role_hierarchy_and_default_deny() {
    let engine = Engine::new(
        vec![
            Role {
                name: String::from("viewer"),
                inherits: vec![],
                permissions: vec![Permission {
                    resource: String::from("article"),
                    action: String::from("read"),
                }],
            },
            Role {
                name: String::from("editor"),
                inherits: vec![String::from("viewer")],
                permissions: vec![Permission {
                    resource: String::from("article"),
                    action: String::from("write"),
                }],
            },
        ],
        vec![],
    )
    .unwrap();

    assert!(engine.check(&request("editor", "read", "article")));
    assert!(engine.check(&request("editor", "write", "article")));
    assert!(!engine.check(&request("editor", "delete", "article")));
}

#[test]
fn explicit_deny_overrides_role_grants() {
    let engine = Engine::new(
        vec![Role {
            name: String::from("editor"),
            inherits: vec![],
            permissions: vec![Permission {
                resource: String::from("article"),
                action: String::from("*"),
            }],
        }],
        vec![Policy {
            name: String::from("deny-delete"),
            effect: Effect::Deny,
            actions: vec![String::from("delete")],
            resources: vec![String::from("article")],
            conditions: vec![],
        }],
    )
    .unwrap();

    let allow = engine.authorize(&request("editor", "write", "article"));
    let deny = engine.authorize(&request("editor", "delete", "article"));
    assert!(allow.allowed);
    assert!(!deny.allowed);
    assert_eq!(deny.reason, "denied by policy");
}

#[test]
fn abac_rules_compare_attributes() {
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

    let mut request = request("ignored", "read", "invoice");
    request.subject.roles.clear();
    request
        .subject
        .attributes
        .insert(String::from("tenant_id"), json!("acme"));
    request
        .resource
        .attributes
        .insert(String::from("tenant_id"), json!("acme"));
    assert!(engine.check(&request));

    request
        .resource
        .attributes
        .insert(String::from("tenant_id"), json!("other"));
    assert!(!engine.check(&request));
}
