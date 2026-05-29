//! Explicit cache backend registry.

use std::collections::BTreeMap;
use std::sync::Arc;
use std::time::Duration;

use rskit_errors::{AppError, AppResult, ErrorCode};

use crate::config::CacheConfig;

/// Minimal async cache operations shared by all backends.
#[async_trait::async_trait]
pub trait CacheBackend: Send + Sync {
    /// Retrieve a string value by key.
    async fn get(&self, key: &str) -> AppResult<Option<String>>;
    /// Store a string value with an optional TTL.
    ///
    /// `Duration::ZERO` is invalid. Backends should honor sub-second TTLs with at least
    /// millisecond precision; durations below one millisecond may be rounded up.
    async fn set(&self, key: &str, val: &str, ttl: Option<Duration>) -> AppResult<()>;
    /// Delete a key and report whether it existed.
    async fn delete(&self, key: &str) -> AppResult<bool>;
    /// Check whether a key currently exists.
    async fn exists(&self, key: &str) -> AppResult<bool>;
}

/// Factory for a named cache backend.
#[async_trait::async_trait]
pub trait CacheFactory: Send + Sync {
    /// Build a backend from cache configuration.
    async fn create(&self, config: &CacheConfig) -> AppResult<Arc<dyn CacheBackend>>;
}

/// Explicit cache backend registry.
#[derive(Default)]
pub struct CacheRegistry {
    factories: BTreeMap<String, Arc<dyn CacheFactory>>,
}

impl CacheRegistry {
    /// Create an empty registry. No backends are registered implicitly.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Register a backend factory under `name`.
    pub fn register(
        &mut self,
        name: impl Into<String>,
        factory: Arc<dyn CacheFactory>,
    ) -> AppResult<()> {
        let name = name.into().trim().to_owned();
        if name.is_empty() {
            return Err(AppError::new(
                ErrorCode::InvalidInput,
                "cache backend name is required",
            ));
        }
        if self.factories.contains_key(&name) {
            return Err(AppError::new(
                ErrorCode::AlreadyExists,
                format!("cache backend '{name}' is already registered"),
            ));
        }
        self.factories.insert(name, factory);
        Ok(())
    }

    /// Return true when `name` is registered.
    #[must_use]
    pub fn contains(&self, name: &str) -> bool {
        self.factories.contains_key(name)
    }

    /// Number of registered backends.
    #[must_use]
    pub fn len(&self) -> usize {
        self.factories.len()
    }

    /// Return true when no backends are registered.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.factories.is_empty()
    }

    /// Build the backend selected by [`CacheConfig::backend`].
    pub async fn build(&self, config: &CacheConfig) -> AppResult<Arc<dyn CacheBackend>> {
        let backend = config.backend.trim();
        if backend.is_empty() {
            return Err(AppError::new(
                ErrorCode::InvalidInput,
                "cache backend name is required",
            ));
        }
        let factory = self.factories.get(backend).ok_or_else(|| {
            AppError::new(
                ErrorCode::NotFound,
                format!("cache backend '{backend}' is not registered"),
            )
        })?;
        factory.create(config).await
    }
}
