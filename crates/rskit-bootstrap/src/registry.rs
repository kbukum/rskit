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

    pub fn len(&self) -> usize {
        self.components.len()
    }

    pub fn is_empty(&self) -> bool {
        self.components.is_empty()
    }
}
