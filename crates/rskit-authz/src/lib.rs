//! Canonical RBAC + ABAC authorization engine with deny-override semantics.

#![warn(missing_docs)]

/// Core authorization checker trait.
pub mod checker;
/// Canonical authorization engine.
pub mod engine;
/// Wildcard permission matching.
pub mod matcher;

pub use checker::Checker;
pub use engine::{
    AttributeSource, Attributes, Condition, Decision, Effect, Engine, Operator, Permission, Policy,
    Request, Resource, Role, Subject,
};
pub use matcher::{match_any, match_pattern};
