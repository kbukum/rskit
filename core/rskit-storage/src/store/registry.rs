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
    #[serde(default = "default_backend")]
    pub backend: String,
    /// Local filesystem backend options.
    #[serde(default)]
    pub local: LocalStoreConfig,
}

impl Default for StorageConfig {
    fn default() -> Self {
        Self {
            backend: default_backend(),
            local: LocalStoreConfig::default(),
        }
    }
}

fn default_backend() -> String {
    "local".to_owned()
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
        let name = name.into().trim().to_owned();
        if name.is_empty() {
            return Err(AppError::new(
                ErrorCode::InvalidInput,
                "storage backend name is required",
            ));
        }
        if self.factories.contains_key(&name) {
            return Err(AppError::new(
                ErrorCode::AlreadyExists,
                format!("storage backend '{name}' is already registered"),
            ));
        }
        self.factories.insert(name, factory);
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
        let backend = config.backend.trim();
        if backend.is_empty() {
            return Err(AppError::new(
                ErrorCode::InvalidInput,
                "storage backend name is required",
            ));
        }
        self.factories
            .get(backend)
            .ok_or_else(|| {
                AppError::new(
                    ErrorCode::NotFound,
                    format!("storage backend '{backend}' is not registered"),
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
        Ok(Arc::new(LocalStore::new(config.local.clone())?))
    }
}

/// Explicitly register the local filesystem backend.
pub fn register_local(registry: &mut StorageRegistry) -> AppResult<()> {
    registry.register("local", Arc::new(LocalFactory))
}
