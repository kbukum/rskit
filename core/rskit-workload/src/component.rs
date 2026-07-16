//! Lifecycle-managed workload component.
//!
//! Mirrors gokit's `workload.Component`: wraps a [`Manager`] built from an
//! injected [`WorkloadRegistry`] and participates in ordered startup, shutdown,
//! and health reporting through [`rskit_component::Component`].

use std::sync::Arc;

use async_trait::async_trait;
use parking_lot::Mutex;
use rskit_component::{Component, Health};
use rskit_errors::AppResult;
use tracing::{debug, info};

use crate::config::WorkloadConfig;
use crate::manager::Manager;
use crate::registry::WorkloadRegistry;

/// A lifecycle-managed workload component.
///
/// On start it builds the configured backend from the injected registry; on stop
/// it releases the manager. When [`WorkloadConfig::enabled`] is `false` the
/// component is a healthy no-op and never touches the registry.
pub struct WorkloadComponent {
    config: WorkloadConfig,
    providers: WorkloadRegistry,
    manager: Mutex<Option<Arc<dyn Manager>>>,
}

impl WorkloadComponent {
    /// Create a component with an empty provider registry.
    ///
    /// Useful when the component is disabled; register providers via
    /// [`WorkloadComponent::with_registry`] to run an enabled backend.
    #[must_use]
    pub fn new(config: WorkloadConfig) -> Self {
        Self::with_registry(config, WorkloadRegistry::new())
    }

    /// Create a component with an explicit provider registry.
    #[must_use]
    pub fn with_registry(config: WorkloadConfig, providers: WorkloadRegistry) -> Self {
        Self {
            config,
            providers,
            manager: Mutex::new(None),
        }
    }

    /// Return the underlying manager once the component has started, or `None`.
    #[must_use]
    pub fn manager(&self) -> Option<Arc<dyn Manager>> {
        self.manager.lock().clone()
    }
}

#[async_trait]
impl Component for WorkloadComponent {
    #[allow(clippy::unnecessary_literal_bound)] // trait fixes the return type to &str
    fn name(&self) -> &str {
        "workload"
    }

    async fn start(&self) -> AppResult<()> {
        let mut config = self.config.clone();
        config.apply_defaults();

        if !config.enabled {
            debug!("workload component disabled — skipping backend init");
            return Ok(());
        }

        let manager = self.providers.build(&config).await?;
        info!(provider = %config.provider, "workload manager initialized");
        *self.manager.lock() = Some(manager);
        Ok(())
    }

    async fn stop(&self) -> AppResult<()> {
        debug!("workload component stopping");
        *self.manager.lock() = None;
        Ok(())
    }

    fn health(&self) -> Health {
        if !self.config.enabled {
            return Health::healthy("workload (disabled)");
        }
        if self.manager.lock().is_none() {
            return Health::unhealthy("workload", "manager not initialized");
        }
        Health::healthy("workload")
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_support::{FailingFactory, FakeFactory};
    use rskit_errors::ErrorCode;

    fn enabled_registry(
        factory: Arc<dyn crate::registry::ManagerFactory>,
    ) -> (WorkloadConfig, WorkloadRegistry) {
        let mut registry = WorkloadRegistry::new();
        registry.register("docker", factory).unwrap();
        let config = WorkloadConfig {
            enabled: true,
            provider: "docker".to_string(),
            ..Default::default()
        };
        (config, registry)
    }

    #[tokio::test]
    async fn disabled_component_starts_healthy_without_manager() {
        let component = WorkloadComponent::new(WorkloadConfig::default());
        assert_eq!(component.name(), "workload");
        component.start().await.unwrap();
        assert!(component.manager().is_none());
        assert!(component.health().is_healthy());
        component.stop().await.unwrap();
    }

    #[tokio::test]
    async fn enabled_component_builds_manager_and_releases_on_stop() {
        let (config, registry) = enabled_registry(Arc::new(FakeFactory));
        let component = WorkloadComponent::with_registry(config, registry);

        component.start().await.unwrap();
        assert!(component.manager().is_some());
        assert!(component.health().is_healthy());

        component.stop().await.unwrap();
        assert!(component.manager().is_none());
        assert!(!component.health().is_healthy());
    }

    #[tokio::test]
    async fn enabled_component_before_start_is_unhealthy() {
        let (config, registry) = enabled_registry(Arc::new(FakeFactory));
        let component = WorkloadComponent::with_registry(config, registry);
        assert!(!component.health().is_healthy());
    }

    #[tokio::test]
    async fn start_propagates_backend_build_failure() {
        let (config, registry) = enabled_registry(Arc::new(FailingFactory));
        let component = WorkloadComponent::with_registry(config, registry);
        let err = component.start().await.unwrap_err();
        assert_eq!(err.code(), ErrorCode::Internal);
        assert!(component.manager().is_none());
    }

    #[tokio::test]
    async fn enabled_component_with_unregistered_provider_fails_to_start() {
        let config = WorkloadConfig {
            enabled: true,
            provider: "podman".to_string(),
            ..Default::default()
        };
        let component = WorkloadComponent::with_registry(config, WorkloadRegistry::new());
        assert_eq!(
            component.start().await.unwrap_err().code(),
            ErrorCode::NotFound
        );
    }
}
