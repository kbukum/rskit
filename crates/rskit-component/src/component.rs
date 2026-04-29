use std::sync::Arc;

use parking_lot::Mutex;
use rskit_errors::AppResult;

use crate::Health;

/// Lifecycle-managed infrastructure component.
///
/// Implement this trait for databases, caches, servers, message brokers, and
/// other infrastructure that must participate in ordered startup, shutdown, and
/// health reporting.
#[async_trait::async_trait]
pub trait Component: Send + Sync {
    /// Stable identifier used in logs and health responses.
    fn name(&self) -> &str;

    /// Start the component.
    async fn start(&self) -> AppResult<()>;

    /// Stop the component gracefully.
    async fn stop(&self) -> AppResult<()>;

    /// Return an instantaneous health snapshot.
    fn health(&self) -> Health;
}

/// Wraps a component factory so construction is deferred until `start()`.
pub struct LazyComponent<F> {
    name: &'static str,
    factory: F,
    inner: Mutex<Option<Arc<dyn Component>>>,
}

impl<F: Fn() -> Arc<dyn Component> + Send + Sync> LazyComponent<F> {
    /// Create a new lazy component with the given `name` and `factory`.
    #[must_use]
    pub fn new(name: &'static str, factory: F) -> Self {
        Self {
            name,
            factory,
            inner: Mutex::new(None),
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
            if let Some(component) = guard.as_ref() {
                Arc::clone(component)
            } else {
                let component = (self.factory)();
                *guard = Some(Arc::clone(&component));
                component
            }
        };
        component.start().await
    }

    async fn stop(&self) -> AppResult<()> {
        let component = self.inner.lock().clone();
        if let Some(component) = component {
            component.stop().await
        } else {
            Ok(())
        }
    }

    fn health(&self) -> Health {
        let component = self.inner.lock().clone();
        if let Some(component) = component {
            component.health()
        } else {
            Health::healthy(self.name)
        }
    }
}
