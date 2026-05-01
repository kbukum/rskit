//! Multiplexed keyed accumulators.

use std::collections::HashMap;
use std::hash::Hash;
use std::sync::Arc;

use parking_lot::Mutex;
use rskit_errors::AppResult;

use crate::{Accumulator, AccumulatorConfig, Store};

type StoreFactory<V> = dyn Fn() -> Box<dyn Store<V>> + Send + Sync;

/// Manages per-key accumulators and TTL cleanup.
pub struct Manager<K, V>
where
    K: Eq + Hash + Clone,
    V: Clone + Send + Sync + 'static,
{
    accumulators: Mutex<HashMap<K, Arc<Accumulator<V>>>>,
    config: AccumulatorConfig<V>,
    store_factory: Arc<StoreFactory<V>>,
}

impl<K, V> Manager<K, V>
where
    K: Eq + Hash + Clone,
    V: Clone + Send + Sync + 'static,
{
    /// Create a new keyed manager.
    #[must_use]
    pub fn new(
        config: AccumulatorConfig<V>,
        store_factory: impl Fn() -> Box<dyn Store<V>> + Send + Sync + 'static,
    ) -> Self {
        Self {
            accumulators: Mutex::new(HashMap::new()),
            config,
            store_factory: Arc::new(store_factory),
        }
    }

    /// Get the accumulator for `key`, creating it when missing.
    pub fn get_or_create(&self, key: K) -> Arc<Accumulator<V>> {
        let mut accumulators = self.accumulators.lock();
        Arc::clone(accumulators.entry(key).or_insert_with(|| {
            Arc::new(Accumulator::new(
                (self.store_factory)(),
                self.config.clone(),
            ))
        }))
    }

    /// Append a value to the keyed accumulator.
    pub fn append(&self, key: K, value: V) -> AppResult<Option<Vec<V>>> {
        self.get_or_create(key).append(value)
    }

    /// Flush a keyed accumulator.
    pub fn flush(&self, key: &K) -> AppResult<Option<Vec<V>>> {
        match self.accumulators.lock().get(key).cloned() {
            Some(accumulator) => Ok(Some(accumulator.flush()?)),
            None => Ok(None),
        }
    }

    /// Return the buffered size for `key`.
    pub fn size(&self, key: &K) -> AppResult<usize> {
        match self.accumulators.lock().get(key).cloned() {
            Some(accumulator) => accumulator.size(),
            None => Ok(0),
        }
    }

    /// List all currently active keys.
    #[must_use]
    pub fn keys(&self) -> Vec<K> {
        self.accumulators.lock().keys().cloned().collect()
    }

    /// Remove expired accumulators and return the removed keys.
    pub fn cleanup_expired(&self) -> AppResult<Vec<K>> {
        let snapshot: Vec<(K, Arc<Accumulator<V>>)> = self
            .accumulators
            .lock()
            .iter()
            .map(|(key, accumulator)| (key.clone(), Arc::clone(accumulator)))
            .collect();

        let mut removed = Vec::new();
        for (key, accumulator) in snapshot {
            if accumulator.is_expired()? {
                accumulator.close()?;
                self.accumulators.lock().remove(&key);
                removed.push(key);
            }
        }
        Ok(removed)
    }
}

#[cfg(test)]
mod tests {
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::sync::{Arc, Barrier};
    use std::time::Duration;

    use super::*;
    use crate::{AccumulatorConfig, MemoryStore, SizeTrigger};

    #[test]
    fn manager_routes_values_by_key() {
        let manager = Manager::new(AccumulatorConfig::new(), || {
            Box::new(MemoryStore::<i32>::new())
        });
        manager.append("a", 1).unwrap();
        manager.append("b", 2).unwrap();
        assert_eq!(manager.size(&"a").unwrap(), 1);
        assert_eq!(manager.size(&"b").unwrap(), 1);
    }

    #[tokio::test]
    async fn manager_cleans_up_expired_accumulators() {
        tokio::time::pause();
        let manager = Manager::new(
            AccumulatorConfig::new().with_ttl(Duration::from_secs(5)),
            || Box::new(MemoryStore::<i32>::new()),
        );
        manager.append("a", 1).unwrap();
        tokio::time::advance(Duration::from_secs(6)).await;
        assert_eq!(manager.cleanup_expired().unwrap(), vec!["a"]);
    }

    #[test]
    fn manager_propagates_flushes() {
        let manager = Manager::new(
            AccumulatorConfig::new().with_trigger(Arc::new(SizeTrigger::new(2))),
            || Box::new(MemoryStore::<i32>::new()),
        );
        assert!(manager.append("a", 1).unwrap().is_none());
        assert_eq!(manager.append("a", 2).unwrap(), Some(vec![1, 2]));
    }

    #[test]
    fn get_or_create_invokes_factory_once_under_contention() {
        const WORKERS: usize = 16;

        let factory_calls = Arc::new(AtomicUsize::new(0));
        let manager = Arc::new(Manager::new(AccumulatorConfig::new(), {
            let factory_calls = Arc::clone(&factory_calls);
            move || {
                factory_calls.fetch_add(1, Ordering::SeqCst);
                Box::new(MemoryStore::<i32>::new())
            }
        }));
        let barrier = Arc::new(Barrier::new(WORKERS));

        let threads: Vec<_> = (0..WORKERS)
            .map(|_| {
                let manager = Arc::clone(&manager);
                let barrier = Arc::clone(&barrier);
                std::thread::spawn(move || {
                    barrier.wait();
                    manager.get_or_create("shared")
                })
            })
            .collect();

        let accumulators: Vec<_> = threads
            .into_iter()
            .map(|thread| thread.join().unwrap())
            .collect();

        assert_eq!(factory_calls.load(Ordering::SeqCst), 1);
        for accumulator in &accumulators[1..] {
            assert!(Arc::ptr_eq(&accumulators[0], accumulator));
        }
    }
}
