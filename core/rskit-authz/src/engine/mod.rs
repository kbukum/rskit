//! Canonical RBAC + ABAC authorization engine: policy model and runtime evaluation.

mod evaluate;
mod model;

pub use evaluate::Engine;
pub use model::{
    AttributeSource, Attributes, Condition, Decision, Effect, Operator, Permission, Policy,
    Request, Resource, Role, Subject,
};
