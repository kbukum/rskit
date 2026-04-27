//! Generic typed registry backed by `DashMap` for concurrent access.

use dashmap::DashMap;
use std::sync::Arc;

/// A concurrent, type-safe registry mapping keys to `Arc<V>`.
pub struct TypedRegistry<K, V> {
    inner: DashMap<K, Arc<V>>,
}

impl<K, V> Default for TypedRegistry<K, V>
where
    K: Eq + std::hash::Hash,
{
    fn default() -> Self {
        Self {
            inner: DashMap::new(),
        }
    }
}

impl<K, V> TypedRegistry<K, V>
where
    K: Eq + std::hash::Hash,
    V: Send + Sync + 'static,
{
    /// Create an empty registry.
    pub fn new() -> Self {
        Self::default()
    }

    /// Insert a value, replacing any existing entry.
    pub fn insert(&self, key: K, value: V) {
        self.inner.insert(key, Arc::new(value));
    }

    /// Get a value by key.
    pub fn get(&self, key: &K) -> Option<Arc<V>> {
        self.inner.get(key).map(|e| Arc::clone(&*e))
    }

    /// Get an existing entry or initialise with `f`.
    pub fn get_or_init(&self, key: K, f: impl FnOnce() -> V) -> Arc<V> {
        self.inner
            .entry(key)
            .or_insert_with(|| Arc::new(f()))
            .clone()
    }

    /// Remove a value.
    pub fn remove(&self, key: &K) -> Option<Arc<V>> {
        self.inner.remove(key).map(|(_, v)| v)
    }

    /// Number of entries.
    pub fn len(&self) -> usize {
        self.inner.len()
    }

    /// Returns `true` if the registry is empty.
    pub fn is_empty(&self) -> bool {
        self.inner.is_empty()
    }
}
