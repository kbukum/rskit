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
        let name = name.into().trim().to_owned();
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
        let backend = config.backend.trim();
        if backend.is_empty() {
            return Err(AppError::new(
                ErrorCode::InvalidInput,
                "database backend name is required",
            ));
        }
        self.factories
            .get(backend)
            .ok_or_else(|| {
                AppError::new(
                    ErrorCode::NotFound,
                    format!("database backend '{backend}' is not registered"),
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

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn build_rejects_blank_backend() {
        let registry = DatabaseRegistry::new();
        let config = DatabaseConfig {
            backend: "  ".to_owned(),
            ..DatabaseConfig::default()
        };

        let err = registry.build(&config).await.err().unwrap();

        assert_eq!(err.code(), ErrorCode::InvalidInput);
    }

    #[tokio::test]
    async fn build_normalizes_backend_before_lookup() {
        let mut registry = DatabaseRegistry::new();
        register_memory(&mut registry).unwrap();
        let config = DatabaseConfig {
            backend: " memory ".to_owned(),
            ..DatabaseConfig::default()
        };

        let database = registry.build(&config).await.unwrap();

        assert!(Arc::strong_count(&database) >= 1);
    }

    #[tokio::test]
    async fn registry_rejects_invalid_duplicate_and_missing_backends() {
        let mut registry = DatabaseRegistry::new();
        assert!(registry.is_empty());
        assert_eq!(registry.len(), 0);

        assert_eq!(
            registry
                .register(" ", Arc::new(MemoryFactory))
                .unwrap_err()
                .code(),
            ErrorCode::InvalidInput
        );
        register_memory(&mut registry).unwrap();
        assert!(registry.contains("memory"));
        assert_eq!(registry.len(), 1);
        assert_eq!(
            register_memory(&mut registry).unwrap_err().code(),
            ErrorCode::AlreadyExists
        );

        let config = DatabaseConfig {
            backend: "missing".to_owned(),
            ..DatabaseConfig::default()
        };
        let error = match registry.build(&config).await {
            Ok(_) => panic!("missing backend should fail"),
            Err(error) => error,
        };
        assert_eq!(error.code(), ErrorCode::NotFound);
    }
}
