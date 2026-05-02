//! Explicit storage backend registry.

use std::collections::BTreeMap;
use std::sync::Arc;

use rskit_errors::{AppError, AppResult, ErrorCode};
use serde::{Deserialize, Serialize};

use super::{FileStore, LocalStore, LocalStoreConfig};

/// Config-driven storage backend selection.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct StorageConfig {
    /// Backend name looked up in an injected [`StorageRegistry`].
    pub backend: String,
    /// Backend-specific JSON options.
    #[serde(default)]
    pub options: serde_json::Value,
}

/// Async factory for storage backends.
#[async_trait::async_trait]
pub trait StorageFactory: Send + Sync {
    /// Build a storage backend from config.
    async fn create(&self, config: &StorageConfig) -> AppResult<Arc<dyn FileStore>>;
}

/// Explicit storage backend registry.
#[derive(Default)]
pub struct StorageRegistry {
    factories: BTreeMap<String, Arc<dyn StorageFactory>>,
}

impl StorageRegistry {
    /// Create an empty registry. No backend is registered implicitly.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Register a backend factory under `name`.
    pub fn register(
        &mut self,
        name: impl Into<String>,
        factory: Arc<dyn StorageFactory>,
    ) -> AppResult<()> {
        let name = name.into();
        if self.factories.insert(name.clone(), factory).is_some() {
            return Err(AppError::new(
                ErrorCode::AlreadyExists,
                format!("storage backend '{name}' is already registered"),
            ));
        }
        Ok(())
    }

    /// Return true when a backend exists in the registry.
    #[must_use]
    pub fn contains(&self, name: &str) -> bool {
        self.factories.contains_key(name)
    }

    /// Number of registered backend factories.
    #[must_use]
    pub fn len(&self) -> usize {
        self.factories.len()
    }

    /// Return true when no backends are registered.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.factories.is_empty()
    }

    /// Build the backend selected by [`StorageConfig::backend`].
    pub async fn build(&self, config: &StorageConfig) -> AppResult<Arc<dyn FileStore>> {
        self.factories
            .get(&config.backend)
            .ok_or_else(|| {
                AppError::new(
                    ErrorCode::NotFound,
                    format!("storage backend '{}' is not registered", config.backend),
                )
            })?
            .create(config)
            .await
    }
}

struct LocalFactory;

#[async_trait::async_trait]
impl StorageFactory for LocalFactory {
    async fn create(&self, config: &StorageConfig) -> AppResult<Arc<dyn FileStore>> {
        let local_config: LocalStoreConfig = serde_json::from_value(config.options.clone())
            .map_err(|e| {
                AppError::new(
                    ErrorCode::InvalidInput,
                    format!("invalid local storage config: {e}"),
                )
            })?;
        Ok(Arc::new(LocalStore::new(local_config)?))
    }
}

/// Explicitly register the local filesystem backend.
pub fn register_local(registry: &mut StorageRegistry) -> AppResult<()> {
    registry.register("local", Arc::new(LocalFactory))
}
