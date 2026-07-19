//! Component lifecycle support for LLM providers.
//!
//! Per locked decision D12,
//! every AI plug (including LLM providers) natively implements `component::Component`.
//! This module provides [`Lifecycle`] —
//! a lightweight mixin that concrete providers embed to satisfy the trait.

use std::time::Instant;

use parking_lot::Mutex;
use rskit_component::{Health, HealthStatus};

/// Shared lifecycle state for LLM provider Component impls.
///
/// Embed as a field and delegate `start`/`stop`/`health` to the corresponding methods here.
#[derive(Debug)]
pub struct Lifecycle {
    name: &'static str,
    inner: Mutex<LifecycleInner>,
}

#[derive(Debug)]
struct LifecycleInner {
    ready: bool,
    last_call: Option<Instant>,
}

impl Lifecycle {
    /// Create a new lifecycle with the given component name.
    #[must_use]
    pub const fn new(name: &'static str) -> Self {
        Self {
            name,
            inner: Mutex::new(LifecycleInner {
                ready: false,
                last_call: None,
            }),
        }
    }

    /// Mark the component as ready (call from `start()`).
    pub fn mark_ready(&self) {
        self.inner.lock().ready = true;
    }

    /// Mark the component as stopped (call from `stop()`).
    pub fn mark_stopped(&self) {
        self.inner.lock().ready = false;
    }

    /// Record a successful call (call after each `complete`/`stream` success).
    pub fn touch(&self) {
        self.inner.lock().last_call = Some(Instant::now());
    }

    /// Whether the component is currently ready.
    #[must_use]
    pub fn is_ready(&self) -> bool {
        self.inner.lock().ready
    }

    /// Produce a [`Health`] snapshot.
    #[must_use]
    pub fn health(&self) -> Health {
        let inner = self.inner.lock();
        let ready = inner.ready;
        let last_call = inner.last_call;
        drop(inner);

        if !ready {
            return Health {
                name: self.name.to_owned(),
                status: HealthStatus::Degraded,
                message: Some(format!("{}: not ready", self.name)),
            };
        }
        let msg = last_call.map_or_else(
            || format!("{}: healthy, no calls yet", self.name),
            |t| format!("{}: healthy, last_call={:?} ago", self.name, t.elapsed()),
        );
        Health {
            name: self.name.to_owned(),
            status: HealthStatus::Healthy,
            message: Some(msg),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lifecycle_starts_not_ready() {
        let lc = Lifecycle::new("test");
        assert!(!lc.is_ready());
        assert!(matches!(lc.health().status, HealthStatus::Degraded));
    }

    #[test]
    fn lifecycle_ready_after_mark() {
        let lc = Lifecycle::new("test");
        lc.mark_ready();
        assert!(lc.is_ready());
        assert!(matches!(lc.health().status, HealthStatus::Healthy));
    }

    #[test]
    fn lifecycle_touch_updates_last_call() {
        let lc = Lifecycle::new("test");
        lc.mark_ready();
        lc.touch();
        let h = lc.health();
        assert!(h.message.unwrap().contains("last_call="));
    }

    #[test]
    fn lifecycle_stopped() {
        let lc = Lifecycle::new("test");
        lc.mark_ready();
        lc.mark_stopped();
        assert!(!lc.is_ready());
    }
}
