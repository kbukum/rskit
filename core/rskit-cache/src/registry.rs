//! Explicit cache store registry.

use std::collections::BTreeMap;
use std::sync::Arc;
use std::time::Duration;

use rskit_errors::{AppError, AppResult, ErrorCode};

use crate::config::CacheConfig;

pub use crate::adapters::memory::register_memory;

/// Minimal async cache storage operations shared by all store adapters.
#[async_trait::async_trait]
pub trait CacheStore: Send + Sync {
    /// Retrieve a string value by key.
    async fn get(&self, key: &str) -> AppResult<Option<String>>;
    /// Store a string value with an optional TTL.
    ///
    /// `Duration::ZERO` is invalid. Stores should honor sub-second TTLs with at least
    /// millisecond precision; durations below one millisecond may be rounded up.
    async fn set(&self, key: &str, val: &str, ttl: Option<Duration>) -> AppResult<()>;
    /// Delete a key and report whether it existed.
    async fn delete(&self, key: &str) -> AppResult<bool>;
    /// Check whether a key currently exists.
    async fn exists(&self, key: &str) -> AppResult<bool>;
}

/// Factory for a named cache store adapter.
#[async_trait::async_trait]
pub trait CacheStoreFactory: Send + Sync {
    /// Build a cache store from cache configuration.
    async fn create(&self, config: &CacheConfig) -> AppResult<Arc<dyn CacheStore>>;
}

/// Explicit cache store registry.
#[derive(Default)]
pub struct CacheRegistry {
    factories: BTreeMap<String, Arc<dyn CacheStoreFactory>>,
}

impl CacheRegistry {
    /// Create an empty registry. No stores are registered implicitly.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Register a store factory under `name`.
    pub fn register(
        &mut self,
        name: impl Into<String>,
        factory: Arc<dyn CacheStoreFactory>,
    ) -> AppResult<()> {
        let name = name.into().trim().to_owned();
        if name.is_empty() {
            return Err(AppError::new(
                ErrorCode::InvalidInput,
                "cache store name is required",
            ));
        }
        if self.factories.contains_key(&name) {
            return Err(AppError::new(
                ErrorCode::AlreadyExists,
                format!("cache store '{name}' is already registered"),
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

    /// Number of registered stores.
    #[must_use]
    pub fn len(&self) -> usize {
        self.factories.len()
    }

    /// Return true when no stores are registered.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.factories.is_empty()
    }

    /// Build the store selected by [`CacheConfig::store`].
    pub async fn build(&self, config: &CacheConfig) -> AppResult<Arc<dyn CacheStore>> {
        let store = config.store.trim();
        if store.is_empty() {
            return Err(AppError::new(
                ErrorCode::InvalidInput,
                "cache store name is required",
            ));
        }
        let factory = self.factories.get(store).ok_or_else(|| {
            AppError::new(
                ErrorCode::NotFound,
                format!("cache store '{store}' is not registered"),
            )
        })?;
        factory.create(config).await
    }
}
