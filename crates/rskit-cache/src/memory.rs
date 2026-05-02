//! In-memory cache backend.

use std::collections::HashMap;
use std::time::{Duration, Instant};

use parking_lot::Mutex;
use rskit_errors::AppResult;

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
}

impl MemoryCache {
    /// Create an empty in-memory cache.
    #[must_use]
    pub fn new(prefix: Option<String>, max_entries: Option<usize>) -> Self {
        Self {
            prefix,
            max_entries,
            entries: Mutex::new(HashMap::new()),
        }
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
            Some(entry) if entry.is_expired(Instant::now()) => {
                entries.remove(&full_key);
                Ok(None)
            }
            Some(entry) => Ok(Some(entry.value.clone())),
            None => Ok(None),
        }
    }

    async fn set(&self, key: &str, val: &str, ttl: Option<Duration>) -> AppResult<()> {
        let mut entries = self.entries.lock();
        let now = Instant::now();
        Self::prune_expired(&mut entries, now);

        if ttl.is_some_and(|ttl| ttl.is_zero()) {
            entries.remove(&self.key(key));
            return Ok(());
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
        Ok(self.entries.lock().remove(&self.key(key)).is_some())
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
