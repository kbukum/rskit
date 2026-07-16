//! Explicit workload backend registry.
//!
//! Backends register a [`ManagerFactory`] under a provider name; the component
//! selects one by [`crate::WorkloadConfig::provider`]. No backend is registered
//! implicitly — construction is always explicit and injected.

use std::collections::BTreeMap;
use std::sync::Arc;

use async_trait::async_trait;
use rskit_errors::{AppError, AppResult, ErrorCode};

use crate::config::WorkloadConfig;
use crate::manager::Manager;

/// Builds a [`Manager`] for a specific backend from workload config.
///
/// Provider-specific settings are captured inside the factory itself, keeping
/// the shared config free of opaque provider data.
#[async_trait]
pub trait ManagerFactory: Send + Sync {
    /// Construct a manager for the given `config`.
    async fn create(&self, config: &WorkloadConfig) -> AppResult<Arc<dyn Manager>>;
}

/// Explicit registry of workload backend factories keyed by provider name.
#[derive(Default)]
pub struct WorkloadRegistry {
    factories: BTreeMap<String, Arc<dyn ManagerFactory>>,
}

impl WorkloadRegistry {
    /// Create an empty registry. No provider is registered implicitly.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Register a backend factory under `name`.
    ///
    /// # Errors
    ///
    /// Returns [`ErrorCode::InvalidInput`] for an empty name and
    /// [`ErrorCode::AlreadyExists`] when the name is already registered.
    pub fn register(
        &mut self,
        name: impl Into<String>,
        factory: Arc<dyn ManagerFactory>,
    ) -> AppResult<()> {
        let name = name.into().trim().to_owned();
        if name.is_empty() {
            return Err(AppError::new(
                ErrorCode::InvalidInput,
                "workload provider name is required",
            ));
        }
        if self.factories.contains_key(&name) {
            return Err(AppError::new(
                ErrorCode::AlreadyExists,
                format!("workload provider '{name}' is already registered"),
            ));
        }
        self.factories.insert(name, factory);
        Ok(())
    }

    /// Return `true` when a provider is registered under `name`.
    #[must_use]
    pub fn contains(&self, name: &str) -> bool {
        self.factories.contains_key(name)
    }

    /// Number of registered providers.
    #[must_use]
    pub fn len(&self) -> usize {
        self.factories.len()
    }

    /// Return `true` when no providers are registered.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.factories.is_empty()
    }

    /// Registered provider names in deterministic (sorted) order.
    #[must_use]
    pub fn names(&self) -> Vec<String> {
        self.factories.keys().cloned().collect()
    }

    /// Build the manager selected by [`WorkloadConfig::provider`].
    ///
    /// # Errors
    ///
    /// Returns [`ErrorCode::MissingField`] when the configured provider is empty
    /// and [`ErrorCode::NotFound`] when it is not registered; otherwise the
    /// factory's own error is propagated.
    pub async fn build(&self, config: &WorkloadConfig) -> AppResult<Arc<dyn Manager>> {
        config.validate()?;
        let provider = config.provider.trim();
        self.factories
            .get(provider)
            .ok_or_else(|| {
                AppError::new(
                    ErrorCode::NotFound,
                    format!("workload provider '{provider}' is not registered"),
                )
            })?
            .create(config)
            .await
    }
}

#[cfg(test)]
mod tests {
    use std::sync::atomic::{AtomicUsize, Ordering};

    use super::*;
    use crate::test_support::FakeManager;

    struct CountingFactory {
        calls: Arc<AtomicUsize>,
    }

    #[async_trait]
    impl ManagerFactory for CountingFactory {
        async fn create(&self, _config: &WorkloadConfig) -> AppResult<Arc<dyn Manager>> {
            self.calls.fetch_add(1, Ordering::SeqCst);
            Ok(Arc::new(FakeManager))
        }
    }

    fn factory(calls: &Arc<AtomicUsize>) -> Arc<CountingFactory> {
        Arc::new(CountingFactory {
            calls: Arc::clone(calls),
        })
    }

    #[tokio::test]
    async fn registers_and_builds_selected_provider() {
        let calls = Arc::new(AtomicUsize::new(0));
        let mut registry = WorkloadRegistry::new();
        assert!(registry.is_empty());

        registry.register(" docker ", factory(&calls)).unwrap();
        assert!(registry.contains("docker"));
        assert_eq!(registry.len(), 1);
        assert_eq!(registry.names(), vec!["docker".to_string()]);

        let config = WorkloadConfig {
            provider: "docker".to_string(),
            ..Default::default()
        };
        registry.build(&config).await.unwrap();
        assert_eq!(calls.load(Ordering::SeqCst), 1);
    }

    #[tokio::test]
    async fn rejects_empty_and_duplicate_names() {
        let calls = Arc::new(AtomicUsize::new(0));
        let mut registry = WorkloadRegistry::new();
        assert_eq!(
            registry.register("  ", factory(&calls)).unwrap_err().code(),
            ErrorCode::InvalidInput
        );
        registry.register("docker", factory(&calls)).unwrap();
        assert_eq!(
            registry
                .register("docker", factory(&calls))
                .unwrap_err()
                .code(),
            ErrorCode::AlreadyExists
        );
    }

    #[tokio::test]
    async fn build_reports_missing_and_unregistered_providers() {
        let registry = WorkloadRegistry::new();
        let empty = WorkloadConfig {
            provider: String::new(),
            ..Default::default()
        };
        assert_eq!(
            registry.build(&empty).await.map(|_| ()).unwrap_err().code(),
            ErrorCode::MissingField
        );

        let unknown = WorkloadConfig {
            provider: "podman".to_string(),
            ..Default::default()
        };
        assert_eq!(
            registry
                .build(&unknown)
                .await
                .map(|_| ())
                .unwrap_err()
                .code(),
            ErrorCode::NotFound
        );
    }
}
