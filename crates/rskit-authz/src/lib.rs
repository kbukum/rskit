//! RBAC and ABAC authorization engine.
//!
//! Provides a [`Checker`] trait and two built-in implementations:
//! - [`RbacChecker`] — role-based access control with wildcard matching.
//! - [`AbacChecker`] — attribute-based access control with pluggable rules.

#![warn(missing_docs)]

/// Attribute-based access control.
pub mod abac;
/// Core authorization checker trait.
pub mod checker;
/// Role-based access control.
pub mod rbac;

pub use abac::{AbacChecker, AbacRule};
pub use checker::Checker;
pub use rbac::{Effect, Policy, RbacChecker};
