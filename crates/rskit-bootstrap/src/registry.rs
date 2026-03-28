use std::sync::Arc;

use rskit_errors::AppResult;

use crate::{Component, Health};

/// Ordered component registry.
///
/// Components start in registration order and stop in reverse — ensuring
/// dependants shut down before their dependencies.
#[derive(Default)]
pub struct Registry {
    components: Vec<Arc<dyn Component>>,
}

impl Registry {
    /// Create an empty [`Registry`].
    pub fn new() -> Self {
        Self::default()
    }

    /// Register a component. Order of registration = order of startup.
    pub fn register(&mut self, c: Arc<dyn Component>) {
        self.components.push(c);
    }

    /// Start all components in registration order.
    pub async fn start_all(&self) -> AppResult<()> {
        for c in &self.components {
            tracing::info!(component = c.name(), "starting component");
            c.start().await?;
            tracing::info!(component = c.name(), "component started");
        }
        Ok(())
    }

    /// Stop all components in reverse registration order (LIFO).
    pub async fn stop_all(&self) {
        for c in self.components.iter().rev() {
            tracing::info!(component = c.name(), "stopping component");
            if let Err(e) = c.stop().await {
                tracing::warn!(component = c.name(), error = %e, "error stopping component");
            }
        }
    }

    /// Collect health for all registered components.
    pub fn health_all(&self) -> Vec<Health> {
        self.components.iter().map(|c| c.health()).collect()
    }

    /// Return the number of registered components.
    pub fn len(&self) -> usize {
        self.components.len()
    }

    /// Return `true` if no components have been registered.
    pub fn is_empty(&self) -> bool {
        self.components.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use std::sync::{
        Arc,
        Mutex,
        atomic::{AtomicUsize, Ordering},
    };

    use rskit_errors::AppError;

    use super::*;
    use crate::Health;

    // ── Mock component ────────────────────────────────────────────────────────

    struct MockComponent {
        name: String,
        start_count: Arc<AtomicUsize>,
        stop_count: Arc<AtomicUsize>,
        healthy: bool,
        /// When set, `start` returns this error message.
        fail_on_start: Option<String>,
        /// Optional shared vec to record the order in which `stop` was called.
        stop_order: Option<Arc<Mutex<Vec<String>>>>,
    }

    impl MockComponent {
        fn new(name: impl Into<String>) -> Self {
            Self {
                name: name.into(),
                start_count: Arc::new(AtomicUsize::new(0)),
                stop_count: Arc::new(AtomicUsize::new(0)),
                healthy: true,
                fail_on_start: None,
                stop_order: None,
            }
        }

        fn with_fail_on_start(mut self, msg: impl Into<String>) -> Self {
            self.fail_on_start = Some(msg.into());
            self
        }

        fn with_stop_order(mut self, order: Arc<Mutex<Vec<String>>>) -> Self {
            self.stop_order = Some(order);
            self
        }
    }

    #[async_trait::async_trait]
    impl Component for MockComponent {
        fn name(&self) -> &str {
            &self.name
        }

        async fn start(&self) -> AppResult<()> {
            if let Some(msg) = &self.fail_on_start {
                return Err(AppError::service_unavailable(msg.clone()));
            }
            self.start_count.fetch_add(1, Ordering::SeqCst);
            Ok(())
        }

        async fn stop(&self) -> AppResult<()> {
            self.stop_count.fetch_add(1, Ordering::SeqCst);
            if let Some(order) = &self.stop_order {
                order.lock().unwrap().push(self.name.clone());
            }
            Ok(())
        }

        fn health(&self) -> Health {
            if self.healthy {
                Health::healthy(&self.name)
            } else {
                Health::unhealthy(&self.name, "mock unhealthy")
            }
        }
    }

    // ── Tests ─────────────────────────────────────────────────────────────────

    #[tokio::test]
    async fn registry_starts_all_in_order() {
        let c1 = Arc::new(MockComponent::new("a"));
        let c2 = Arc::new(MockComponent::new("b"));
        let c3 = Arc::new(MockComponent::new("c"));

        let sc1 = c1.start_count.clone();
        let sc2 = c2.start_count.clone();
        let sc3 = c3.start_count.clone();

        let mut reg = Registry::new();
        reg.register(c1);
        reg.register(c2);
        reg.register(c3);

        reg.start_all().await.expect("start_all should succeed");

        assert_eq!(sc1.load(Ordering::SeqCst), 1, "component 'a' should start once");
        assert_eq!(sc2.load(Ordering::SeqCst), 1, "component 'b' should start once");
        assert_eq!(sc3.load(Ordering::SeqCst), 1, "component 'c' should start once");
    }

    #[tokio::test]
    async fn registry_stops_all_in_reverse_order() {
        let stop_order: Arc<Mutex<Vec<String>>> = Arc::new(Mutex::new(Vec::new()));

        let c1 = Arc::new(MockComponent::new("first").with_stop_order(stop_order.clone()));
        let c2 = Arc::new(MockComponent::new("second").with_stop_order(stop_order.clone()));
        let c3 = Arc::new(MockComponent::new("third").with_stop_order(stop_order.clone()));

        let mut reg = Registry::new();
        reg.register(c1);
        reg.register(c2);
        reg.register(c3);

        reg.start_all().await.expect("start_all should succeed");
        reg.stop_all().await;

        let order = stop_order.lock().unwrap();
        assert_eq!(*order, vec!["third", "second", "first"]);
    }

    #[tokio::test]
    async fn registry_health_all_returns_one_per_component() {
        let mut reg = Registry::new();
        reg.register(Arc::new(MockComponent::new("svc-a")));
        reg.register(Arc::new(MockComponent::new("svc-b")));
        reg.register(Arc::new(MockComponent::new("svc-c")));

        let healths = reg.health_all();
        assert_eq!(healths.len(), 3);
        assert_eq!(healths[0].name, "svc-a");
        assert_eq!(healths[1].name, "svc-b");
        assert_eq!(healths[2].name, "svc-c");
        assert!(healths.iter().all(|h| h.is_healthy()));
    }

    #[tokio::test]
    async fn registry_start_failure_returns_err() {
        // Component at index 1 will fail; start_all should return Err.
        let c1 = Arc::new(MockComponent::new("ok-first"));
        let c2 = Arc::new(MockComponent::new("fail-second").with_fail_on_start("boom"));
        let c3 = Arc::new(MockComponent::new("never-third"));

        let sc3 = c3.start_count.clone();

        let mut reg = Registry::new();
        reg.register(c1);
        reg.register(c2);
        reg.register(c3);

        let result = reg.start_all().await;
        assert!(result.is_err(), "start_all must propagate the component error");
        // The third component must NOT have been started.
        assert_eq!(sc3.load(Ordering::SeqCst), 0, "component after failed one must not start");
    }
}
