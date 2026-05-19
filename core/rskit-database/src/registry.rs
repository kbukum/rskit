//! Explicit database backend registry.

use std::collections::BTreeMap;
use std::sync::Arc;

use rskit_errors::{AppError, AppResult, ErrorCode};

use crate::config::DatabaseConfig;
use crate::database::{DatabaseClient, memory_from_config};

/// Factory for a named database backend.
#[async_trait::async_trait]
pub trait DatabaseFactory: Send + Sync {
    /// Build a database client from database configuration.
    async fn create(&self, config: &DatabaseConfig) -> AppResult<Arc<dyn DatabaseClient>>;
}

/// Explicit database backend registry.
#[derive(Default)]
pub struct DatabaseRegistry {
    factories: BTreeMap<String, Arc<dyn DatabaseFactory>>,
}

impl DatabaseRegistry {
    /// Create an empty database backend registry.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Register a backend factory under `name`.
    pub fn register(
        &mut self,
        name: impl Into<String>,
        factory: Arc<dyn DatabaseFactory>,
    ) -> AppResult<()> {
        let name = name.into();
        if name.is_empty() {
            return Err(AppError::new(
                ErrorCode::InvalidInput,
                "database backend name is required",
            ));
        }
        if self.factories.contains_key(&name) {
            return Err(AppError::new(
                ErrorCode::AlreadyExists,
                format!("database backend '{name}' is already registered"),
            ));
        }
        self.factories.insert(name, factory);
        Ok(())
    }

    /// Return true when the backend has been explicitly registered.
    #[must_use]
    pub fn contains(&self, name: &str) -> bool {
        self.factories.contains_key(name)
    }

    /// Number of registered database backends.
    #[must_use]
    pub fn len(&self) -> usize {
        self.factories.len()
    }

    /// Return true when no backends are registered.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.factories.is_empty()
    }

    /// Build the backend selected by [`DatabaseConfig::backend`].
    pub async fn build(&self, config: &DatabaseConfig) -> AppResult<Arc<dyn DatabaseClient>> {
        self.factories
            .get(&config.backend)
            .ok_or_else(|| {
                AppError::new(
                    ErrorCode::NotFound,
                    format!("database backend '{}' is not registered", config.backend),
                )
            })?
            .create(config)
            .await
    }
}

struct MemoryFactory;

#[async_trait::async_trait]
impl DatabaseFactory for MemoryFactory {
    async fn create(&self, config: &DatabaseConfig) -> AppResult<Arc<dyn DatabaseClient>> {
        Ok(Arc::new(memory_from_config(config)))
    }
}

/// Explicitly register the in-memory backend.
pub fn register_memory(registry: &mut DatabaseRegistry) -> AppResult<()> {
    registry.register("memory", Arc::new(MemoryFactory))
}
