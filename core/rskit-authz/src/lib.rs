//! Canonical RBAC + ABAC authorization engine with deny-override semantics.

#![warn(missing_docs)]

/// Core authorization checker trait.
pub mod checker;
/// Canonical authorization engine.
pub mod engine;
/// Wildcard permission matching.
pub mod matcher;
/// Tower authorization middleware.
pub mod middleware;

pub use checker::Checker;
pub use engine::{
    AttributeSource, Attributes, Condition, Decision, Effect, Engine, Operator, Permission, Policy,
    Request, Resource, Role, Subject,
};
pub use matcher::{match_any, match_pattern};
pub use middleware::{AuthorizationLayer, AuthorizationService, RequestAuthorizer};

/// Transport-agnostic authorization decision.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[non_exhaustive]
pub enum AuthzDecision {
    /// Request is allowed.
    Allow,
    /// Request is denied with a reason.
    Deny(String),
    /// Request requires human approval.
    RequiresHumanApproval(String),
}

/// Transport-agnostic authorization request for a principal, action, and resource.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct AuthzRequest {
    /// Principal identifier.
    pub principal: String,
    /// Action being performed.
    pub action: String,
    /// Resource identifier.
    pub resource: String,
    /// Scopes relevant to the request.
    #[serde(default)]
    pub scopes: Vec<String>,
    /// Additional structured attributes.
    #[serde(default)]
    pub attributes: serde_json::Value,
}

/// Object-safe authorization decider used at integration boundaries.
pub trait Decider: Send + Sync {
    /// Decide one authorization request.
    fn decide(&self, request: &AuthzRequest) -> AuthzDecision;
}
