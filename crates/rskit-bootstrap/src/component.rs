use std::sync::Arc;

use rskit_errors::AppResult;

use crate::Health;

/// Lifecycle-managed infrastructure component.
///
/// Implement this trait for databases, caches, gRPC servers, message queues —
/// any infrastructure that needs ordered start/stop and health reporting.
///
/// # Object safety
///
/// The `#[async_trait]` attribute makes this trait object-safe so components
/// can be stored as `Arc<dyn Component>` in the [`Registry`](crate::Registry).
#[async_trait::async_trait]
pub trait Component: Send + Sync {
    /// Stable identifier used in logs and health responses.
    fn name(&self) -> &str;

    /// Start the component. Called before application hooks.
    async fn start(&self) -> AppResult<()>;

    /// Stop the component gracefully. Called after application stop hooks.
    async fn stop(&self) -> AppResult<()>;

    /// Instant (synchronous) health snapshot.
    fn health(&self) -> Health;
}

// ── LazyComponent ─────────────────────────────────────────────────────────────

/// Wraps a component factory: the inner component is not constructed until
/// `start()` is first called.
///
/// Useful when the concrete component requires I/O during construction (e.g.
/// establishing a connection) and you want to defer that work to startup.
pub struct LazyComponent<F> {
    name: &'static str,
    factory: F,
    inner: parking_lot::Mutex<Option<Arc<dyn Component>>>,
}

impl<F: Fn() -> Arc<dyn Component> + Send + Sync> LazyComponent<F> {
    /// Create a new lazy component with the given `name` and `factory`.
    pub fn new(name: &'static str, factory: F) -> Self {
        Self {
            name,
            factory,
            inner: parking_lot::Mutex::new(None),
        }
    }
}

#[async_trait::async_trait]
impl<F: Fn() -> Arc<dyn Component> + Send + Sync> Component for LazyComponent<F> {
    fn name(&self) -> &str {
        self.name
    }

    async fn start(&self) -> AppResult<()> {
        let component = {
            let mut guard = self.inner.lock();
            if guard.is_none() {
                *guard = Some((self.factory)());
            }
            // SAFETY: set to Some in the branch above; always Some at this point.
            guard.as_ref().expect("just initialized above; qed").clone()
        };
        component.start().await
    }

    async fn stop(&self) -> AppResult<()> {
        let component = self.inner.lock().clone();
        if let Some(c) = component {
            c.stop().await
        } else {
            Ok(())
        }
    }

    fn health(&self) -> Health {
        let component = self.inner.lock().clone();
        if let Some(c) = component {
            c.health()
        } else {
            Health::healthy(self.name)
        }
    }
}
