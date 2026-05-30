//! In-memory cache adapter.

use std::collections::BTreeMap;
use std::sync::Arc;
use std::time::{Duration, Instant};

use parking_lot::Mutex;
use rskit_errors::{AppError, AppResult, ErrorCode};

use crate::config::CacheConfig;
use crate::registry::{CacheRegistry, CacheStore, CacheStoreFactory};

#[derive(Clone)]
struct Entry {
    value: String,
    expires_at: Option<Instant>,
}

impl Entry {
    fn is_expired(&self, now: Instant) -> bool {
        self.expires_at.is_some_and(|expires_at| expires_at <= now)
    }
}

/// Lean in-process cache store adapter for local development and tests.
pub struct MemoryCache {
    prefix: Option<String>,
    max_entries: Option<usize>,
    entries: Mutex<BTreeMap<String, Entry>>,
    clock: Arc<dyn Fn() -> Instant + Send + Sync>,
}

impl MemoryCache {
    /// Create an empty in-memory cache.
    #[must_use]
    pub fn new(prefix: Option<String>, max_entries: Option<usize>) -> Self {
        Self::new_with_clock(prefix, max_entries, Instant::now)
    }

    /// Create an in-memory cache with an injected clock.
    ///
    /// This is primarily useful for deterministic tests and simulations that
    /// need to advance cache expiry without sleeping.
    #[must_use]
    pub fn new_with_clock(
        prefix: Option<String>,
        max_entries: Option<usize>,
        clock: impl Fn() -> Instant + Send + Sync + 'static,
    ) -> Self {
        Self {
            prefix,
            max_entries: max_entries.filter(|entries| *entries > 0),
            entries: Mutex::new(BTreeMap::new()),
            clock: Arc::new(clock),
        }
    }

    fn now(&self) -> Instant {
        (self.clock)()
    }

    fn key(&self, key: &str) -> String {
        self.prefix
            .as_ref()
            .map_or_else(|| key.to_owned(), |prefix| format!("{prefix}:{key}"))
    }

    fn prune_expired(entries: &mut BTreeMap<String, Entry>, now: Instant) {
        entries.retain(|_, entry| !entry.is_expired(now));
    }

    fn expires_at(now: Instant, ttl: Option<Duration>) -> AppResult<Option<Instant>> {
        let Some(duration) = ttl else {
            return Ok(None);
        };
        now.checked_add(duration).map(Some).ok_or_else(|| {
            AppError::new(
                ErrorCode::InvalidInput,
                "cache TTL is too large to represent safely",
            )
        })
    }
}

#[async_trait::async_trait]
impl CacheStore for MemoryCache {
    async fn get(&self, key: &str) -> AppResult<Option<String>> {
        let mut entries = self.entries.lock();
        Self::prune_expired(&mut entries, self.now());
        let full_key = self.key(key);
        Ok(entries.get(&full_key).map(|entry| entry.value.clone()))
    }

    async fn set(&self, key: &str, val: &str, ttl: Option<Duration>) -> AppResult<()> {
        let mut entries = self.entries.lock();
        let now = self.now();
        Self::prune_expired(&mut entries, now);
        let key = self.key(key);

        if ttl.is_some_and(|ttl| ttl.is_zero()) {
            return Err(AppError::new(
                ErrorCode::InvalidInput,
                "cache TTL must be greater than zero",
            ));
        }
        let expires_at = Self::expires_at(now, ttl)?;

        if let Some(max_entries) = self.max_entries
            && entries.len() >= max_entries
            && !entries.contains_key(&key)
            && let Some(first_key) = entries.keys().next().cloned()
        {
            entries.remove(&first_key);
        }

        entries.insert(
            key,
            Entry {
                value: val.to_owned(),
                expires_at,
            },
        );
        Ok(())
    }

    async fn delete(&self, key: &str) -> AppResult<bool> {
        let mut entries = self.entries.lock();
        Self::prune_expired(&mut entries, self.now());
        let key = self.key(key);
        Ok(entries.remove(&key).is_some())
    }

    async fn exists(&self, key: &str) -> AppResult<bool> {
        self.get(key).await.map(|value| value.is_some())
    }
}

impl Default for MemoryCache {
    fn default() -> Self {
        Self::new(None, None)
    }
}

struct MemoryFactory;

#[async_trait::async_trait]
impl CacheStoreFactory for MemoryFactory {
    async fn create(&self, config: &CacheConfig) -> AppResult<Arc<dyn CacheStore>> {
        Ok(Arc::new(MemoryCache::new(
            config.key_prefix.clone(),
            config.memory.max_entries,
        )))
    }
}

/// Explicitly register the in-memory adapter.
pub fn register_memory(registry: &mut CacheRegistry) -> AppResult<()> {
    registry.register("memory", Arc::new(MemoryFactory))
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use parking_lot::Mutex;

    use super::*;

    #[tokio::test]
    async fn set_rejects_unrepresentable_ttl() {
        let cache = MemoryCache::default();

        let err = cache
            .set("too-long", "value", Some(Duration::MAX))
            .await
            .expect_err("TTL overflow must be rejected");

        assert_eq!(err.code, ErrorCode::InvalidInput);
        assert!(err.to_string().contains("too large"));
    }

    #[tokio::test]
    async fn get_prunes_unrelated_expired_entries() {
        let now = Arc::new(Mutex::new(Instant::now()));
        let clock = Arc::clone(&now);
        let cache = MemoryCache::new_with_clock(None, None, move || *clock.lock());
        cache
            .set("expired", "value", Some(Duration::from_secs(1)))
            .await
            .unwrap();
        cache.set("live", "value", None).await.unwrap();
        *now.lock() += Duration::from_secs(2);

        assert_eq!(cache.get("live").await.unwrap().as_deref(), Some("value"));
        assert_eq!(cache.entries.lock().len(), 1);
    }

    #[tokio::test]
    async fn delete_prunes_unrelated_expired_entries() {
        let now = Arc::new(Mutex::new(Instant::now()));
        let clock = Arc::clone(&now);
        let cache = MemoryCache::new_with_clock(None, None, move || *clock.lock());
        cache
            .set("expired", "value", Some(Duration::from_secs(1)))
            .await
            .unwrap();
        cache.set("live", "value", None).await.unwrap();
        *now.lock() += Duration::from_secs(2);

        assert!(!cache.delete("missing").await.unwrap());
        assert_eq!(cache.entries.lock().len(), 1);
    }
}
