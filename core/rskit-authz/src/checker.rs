//! Lightweight authorization traits.

use crate::engine::{Decision, Request};

/// Policy enforcement point for authorization checks.
pub trait Checker: Send + Sync {
    /// Evaluate a request and return the full decision.
    fn authorize(&self, request: &Request) -> Decision;

    /// Return `true` when the request is allowed.
    fn check(&self, request: &Request) -> bool {
        self.authorize(request).allowed
    }
}
