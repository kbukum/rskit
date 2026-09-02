//! Explicit cache store registry.

use std::collections::BTreeMap;
use std::sync::Arc;
use std::time::Duration;

use rskit_errors::{AppError, AppResult, ErrorCode};

use crate::config::CacheConfig;

pub use crate::adapters::memory::register_memory;

/// Minimal async cache storage operations shared by all store adapters.
///
/// Values are opaque byte strings; callers own any encoding (JSON, protobuf, …).
#[async_trait::async_trait]
pub trait CacheStore: Send + Sync {
    /// Retrieve a raw value by key.
    async fn get(&self, key: &str) -> AppResult<Option<Vec<u8>>>;
    /// Store a raw value with an optional TTL.
    ///
    /// `Duration::ZERO` is invalid.
    /// Stores should honor sub-second TTLs with at least millisecond precision;
    /// durations below one millisecond may be rounded up.
    async fn set(&self, key: &str, val: &[u8], ttl: Option<Duration>) -> AppResult<()>;
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

    /// Build the store selected by [`CacheConfig::provider`].
    pub async fn build(&self, config: &CacheConfig) -> AppResult<Arc<dyn CacheStore>> {
        let store = config.provider.trim();
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

#[cfg(test)]
mod tests {
    use super::*;

    #[derive(Default)]
    struct DummyStore;

    #[async_trait::async_trait]
    impl CacheStore for DummyStore {
        async fn get(&self, _key: &str) -> AppResult<Option<Vec<u8>>> {
            Ok(None)
        }

        async fn set(&self, _key: &str, _val: &[u8], _ttl: Option<Duration>) -> AppResult<()> {
            Ok(())
        }

        async fn delete(&self, _key: &str) -> AppResult<bool> {
            Ok(false)
        }

        async fn exists(&self, _key: &str) -> AppResult<bool> {
            Ok(false)
        }
    }

    #[derive(Default)]
    struct DummyFactory;

    #[async_trait::async_trait]
    impl CacheStoreFactory for DummyFactory {
        async fn create(&self, _config: &CacheConfig) -> AppResult<Arc<dyn CacheStore>> {
            Ok(Arc::new(DummyStore))
        }
    }

    #[test]
    fn register_rejects_empty_store_name() {
        let mut registry = CacheRegistry::new();
        let err = registry
            .register(" \t ", Arc::new(DummyFactory))
            .expect_err("empty store names must be rejected");
        assert_eq!(err.code(), ErrorCode::InvalidInput);
    }

    #[test]
    fn register_rejects_duplicate_store_name() {
        let mut registry = CacheRegistry::new();
        registry
            .register("memory", Arc::new(DummyFactory))
            .expect("first registration should succeed");

        let err = registry
            .register("memory", Arc::new(DummyFactory))
            .expect_err("duplicate store names must be rejected");
        assert_eq!(err.code(), ErrorCode::AlreadyExists);
    }

    #[tokio::test]
    async fn build_rejects_empty_selected_store() {
        let registry = CacheRegistry::new();
        let config = CacheConfig {
            provider: " ".into(),
            ..CacheConfig::default()
        };

        assert_eq!(
            registry.build(&config).await.err().map(|err| err.code()),
            Some(ErrorCode::InvalidInput)
        );
    }

    #[tokio::test]
    async fn build_rejects_unregistered_selected_store() {
        let registry = CacheRegistry::new();
        let config = CacheConfig {
            provider: "redis".into(),
            ..CacheConfig::default()
        };

        assert_eq!(
            registry.build(&config).await.err().map(|err| err.code()),
            Some(ErrorCode::NotFound)
        );
    }

    #[tokio::test]
    async fn build_uses_registered_factory() {
        let mut registry = CacheRegistry::new();
        registry
            .register("memory", Arc::new(DummyFactory))
            .expect("registration should succeed");
        assert!(registry.contains("memory"));
        assert_eq!(registry.len(), 1);
        assert!(!registry.is_empty());

        let config = CacheConfig {
            provider: " memory ".into(),
            ..CacheConfig::default()
        };

        let store = registry
            .build(&config)
            .await
            .expect("registered store should build");
        assert!(
            store
                .get("missing")
                .await
                .expect("get should succeed")
                .is_none()
        );
        store
            .set("k", b"v", None)
            .await
            .expect("set should succeed");
        assert!(!store.delete("k").await.expect("delete should succeed"));
        assert!(
            !store
                .exists("missing")
                .await
                .expect("exists should succeed")
        );
    }
}
