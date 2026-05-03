//! In-memory cache backend.

use std::collections::HashMap;
use std::sync::Arc;
use std::time::{Duration, Instant};

use parking_lot::Mutex;
use rskit_errors::{AppError, AppResult, ErrorCode};

use crate::registry::CacheBackend;

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

/// Lean in-process cache backend used as the default core implementation.
pub struct MemoryCache {
    prefix: Option<String>,
    max_entries: Option<usize>,
    entries: Mutex<HashMap<String, Entry>>,
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
            max_entries,
            entries: Mutex::new(HashMap::new()),
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

    fn prune_expired(entries: &mut HashMap<String, Entry>, now: Instant) {
        entries.retain(|_, entry| !entry.is_expired(now));
    }
}

#[async_trait::async_trait]
impl CacheBackend for MemoryCache {
    async fn get(&self, key: &str) -> AppResult<Option<String>> {
        let mut entries = self.entries.lock();
        let full_key = self.key(key);
        match entries.get(&full_key) {
            Some(entry) if entry.is_expired(self.now()) => {
                entries.remove(&full_key);
                Ok(None)
            }
            Some(entry) => Ok(Some(entry.value.clone())),
            None => Ok(None),
        }
    }

    async fn set(&self, key: &str, val: &str, ttl: Option<Duration>) -> AppResult<()> {
        let mut entries = self.entries.lock();
        let now = self.now();
        Self::prune_expired(&mut entries, now);

        if ttl.is_some_and(|ttl| ttl.is_zero()) {
            return Err(AppError::new(
                ErrorCode::InvalidInput,
                "cache TTL must be greater than zero",
            ));
        }

        if let Some(max_entries) = self.max_entries
            && entries.len() >= max_entries
            && !entries.contains_key(&self.key(key))
            && let Some(first_key) = entries.keys().next().cloned()
        {
            entries.remove(&first_key);
        }

        entries.insert(
            self.key(key),
            Entry {
                value: val.to_owned(),
                expires_at: ttl.map(|duration| now + duration),
            },
        );
        Ok(())
    }

    async fn delete(&self, key: &str) -> AppResult<bool> {
        let mut entries = self.entries.lock();
        let now = self.now();
        let key = self.key(key);
        if entries
            .get(&key)
            .and_then(|entry| entry.expires_at)
            .is_some_and(|expires_at| expires_at <= now)
        {
            entries.remove(&key);
            return Ok(false);
        }
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
