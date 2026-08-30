//! Multiplexed keyed accumulators.

use std::collections::HashMap;
use std::hash::Hash;
use std::sync::Arc;

use parking_lot::Mutex;
use rskit_errors::{AppError, AppResult, ErrorCode};

use crate::{Accumulator, AccumulatorConfig, Store};

type StoreFactory<V> = dyn Fn() -> Box<dyn Store<V>> + Send + Sync;

/// Outcome of a [`Manager::cleanup_expired`] sweep.
///
/// Carries both the keys removed from the manager and any aggregated teardown error, so a failing store `close()` neither hides which entries were evicted nor aborts the sweep. Marked `#[must_use]` so the report is not accidentally discarded.
#[must_use]
#[non_exhaustive]
pub struct CleanupReport<K> {
    /// Keys removed from the manager during the sweep.
    pub removed: Vec<K>,
    /// Aggregated teardown error, if one or more accumulators failed to close (or their expiry check failed).
    ///
    /// The first failure is surfaced as-is, preserving its code, cause, and details; the messages of any further failures are attached under the `additional_close_errors` detail and summarized in a hint.
    pub error: Option<AppError>,
}

impl<K> CleanupReport<K> {
    /// Consume the report, returning the removed keys when the sweep fully succeeded, or the aggregated error otherwise.
    pub fn into_result(self) -> AppResult<Vec<K>> {
        match self.error {
            Some(error) => Err(error),
            None => Ok(self.removed),
        }
    }
}

/// Combine teardown failures into a single error. The first failure is returned as-is — preserving its code, retryability, cause, and structured details — with the messages of any further failures attached under `additional_close_errors` and summarized in a hint. Returns `None` when nothing failed.
fn aggregate_errors(errors: Vec<AppError>) -> Option<AppError> {
    let mut iter = errors.into_iter();
    let first = iter.next()?;
    let rest: Vec<String> = iter.map(|err| err.message().to_string()).collect();
    if rest.is_empty() {
        return Some(first);
    }
    let summary = format!(
        "(and {} more error(s) during teardown: {})",
        rest.len(),
        rest.join("; ")
    );
    Some(
        first
            .with_detail("additional_close_errors", rest)
            .hint(summary),
    )
}

/// Internal manager state guarded by a single mutex.
struct ManagerState<K, V: Clone> {
    accumulators: HashMap<K, Arc<Accumulator<V>>>,
    closed: bool,
}

/// Manages per-key accumulators and TTL cleanup.
pub struct Manager<K, V>
where
    K: Eq + Hash + Clone,
    V: Clone + Send + Sync + 'static,
{
    state: Mutex<ManagerState<K, V>>,
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
            state: Mutex::new(ManagerState {
                accumulators: HashMap::new(),
                closed: false,
            }),
            config,
            store_factory: Arc::new(store_factory),
        }
    }

    /// Get the accumulator for `key`, creating it when missing.
    ///
    /// Returns a [`ErrorCode::Conflict`] error once the manager has been closed via [`Manager::close`], so a shutdown cannot be silently undone by resurrecting a fresh, unmanaged accumulator whose store would never be closed.
    pub fn get_or_create(&self, key: K) -> AppResult<Arc<Accumulator<V>>> {
        let mut state = self.state.lock();
        if state.closed {
            return Err(AppError::new(
                ErrorCode::Conflict,
                "stateful manager is closed",
            ));
        }
        Ok(Arc::clone(state.accumulators.entry(key).or_insert_with(
            || {
                Arc::new(Accumulator::new(
                    (self.store_factory)(),
                    self.config.clone(),
                ))
            },
        )))
    }

    /// Append a value to the keyed accumulator.
    pub fn append(&self, key: K, value: V) -> AppResult<Option<Vec<V>>> {
        self.get_or_create(key)?.append(value)
    }

    /// Flush a keyed accumulator.
    pub fn flush(&self, key: &K) -> AppResult<Option<Vec<V>>> {
        match self.state.lock().accumulators.get(key).cloned() {
            Some(accumulator) => Ok(Some(accumulator.flush()?)),
            None => Ok(None),
        }
    }

    /// Return the buffered size for `key`.
    pub fn size(&self, key: &K) -> AppResult<usize> {
        match self.state.lock().accumulators.get(key).cloned() {
            Some(accumulator) => accumulator.size(),
            None => Ok(0),
        }
    }

    /// List all currently active keys.
    #[must_use]
    pub fn keys(&self) -> Vec<K> {
        self.state.lock().accumulators.keys().cloned().collect()
    }

    /// Remove expired accumulators, closing each and reporting the outcome.
    ///
    /// Every expired accumulator is closed and removed from the manager, even when an earlier `close()` fails — a single failure neither aborts the sweep nor leaves a partially-closed entry stuck in the map. Each entry is claimed under the lock before being closed, so concurrent sweeps (or a racing [`Manager::close`]) never double-close an accumulator and only the caller that actually removed it reports it. The returned [`CleanupReport`] carries the removed keys and any aggregated error (see [`CleanupReport::error`]).
    pub fn cleanup_expired(&self) -> CleanupReport<K> {
        let candidates: Vec<(K, Arc<Accumulator<V>>)> = {
            let state = self.state.lock();
            state
                .accumulators
                .iter()
                .map(|(key, accumulator)| (key.clone(), Arc::clone(accumulator)))
                .collect()
        };

        let mut removed = Vec::new();
        let mut errors = Vec::new();
        for (key, accumulator) in candidates {
            match accumulator.is_expired() {
                Ok(true) => {
                    let claimed = self.state.lock().accumulators.remove(&key);
                    if let Some(accumulator) = claimed {
                        if let Err(err) = accumulator.close() {
                            errors.push(err);
                        }
                        removed.push(key);
                    }
                }
                Ok(false) => {}
                Err(err) => errors.push(err),
            }
        }
        CleanupReport {
            removed,
            error: aggregate_errors(errors),
        }
    }

    /// Close every held accumulator and mark the manager closed.
    ///
    /// Under the state lock the manager is marked closed and its accumulator map is drained atomically, so no concurrent [`Manager::get_or_create`] can insert a fresh accumulator after the drain and escape teardown. Each drained accumulator is then closed exactly once outside the lock (including non-expired ones whose store `close()` side effects would otherwise be skipped when the manager is dropped); teardown is never short-circuited by an earlier failure. When one or more closes fail, the first error is surfaced with the messages of the rest aggregated under the `additional_close_errors` detail. The manager is inert afterward — subsequent `get_or_create`/`append` calls are rejected — and a second `close()` is a no-op returning `Ok`.
    pub fn close(&self) -> AppResult<()> {
        let drained: Vec<Arc<Accumulator<V>>> = {
            let mut state = self.state.lock();
            state.closed = true;
            state
                .accumulators
                .drain()
                .map(|(_, accumulator)| accumulator)
                .collect()
        };

        let mut errors = Vec::new();
        for accumulator in drained {
            if let Err(err) = accumulator.close() {
                errors.push(err);
            }
        }
        match aggregate_errors(errors) {
            Some(error) => Err(error),
            None => Ok(()),
        }
    }
}

#[cfg(test)]
mod tests {
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::sync::{Arc, Barrier};
    use std::time::Duration;

    use rskit_errors::ErrorCode;

    use super::*;
    use crate::test_support::CountingStore;
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
        assert_eq!(manager.cleanup_expired().into_result().unwrap(), vec!["a"]);
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
                    manager.get_or_create("shared").unwrap()
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

    #[test]
    fn flush_size_and_keys_handle_missing_and_existing_keys() {
        let manager = Manager::new(AccumulatorConfig::new(), || {
            Box::new(MemoryStore::<i32>::new())
        });

        assert_eq!(manager.flush(&"missing").unwrap(), None);
        assert_eq!(manager.size(&"missing").unwrap(), 0);
        manager.append("a", 1).unwrap();

        assert_eq!(manager.keys(), vec!["a"]);
        assert_eq!(manager.flush(&"a").unwrap(), Some(vec![1]));
    }

    #[test]
    fn close_closes_every_held_accumulator_once() {
        let close_calls = Arc::new(AtomicUsize::new(0));
        let manager = Manager::new(AccumulatorConfig::new(), {
            let close_calls = Arc::clone(&close_calls);
            move || Box::new(CountingStore::<i32>::new(Arc::clone(&close_calls), false))
        });

        manager.append("a", 1).unwrap();
        manager.append("b", 2).unwrap();
        manager.append("c", 3).unwrap();

        assert!(manager.close().is_ok());
        assert_eq!(close_calls.load(Ordering::SeqCst), 3);
        assert!(manager.keys().is_empty());
    }

    #[test]
    fn close_surfaces_error_but_still_closes_all() {
        let close_calls = Arc::new(AtomicUsize::new(0));
        let manager = Manager::new(AccumulatorConfig::new(), {
            let close_calls = Arc::clone(&close_calls);
            move || Box::new(CountingStore::<i32>::new(Arc::clone(&close_calls), true))
        });

        manager.append("a", 1).unwrap();
        manager.append("b", 2).unwrap();

        let error = manager.close().unwrap_err();
        assert_eq!(error.code(), ErrorCode::Internal);
        let extra = error
            .details()
            .get("additional_close_errors")
            .and_then(|value| value.as_array())
            .map(Vec::len);
        assert_eq!(extra, Some(1));
        assert_eq!(close_calls.load(Ordering::SeqCst), 2);
        assert!(manager.keys().is_empty());
    }

    #[test]
    fn close_is_idempotent() {
        let close_calls = Arc::new(AtomicUsize::new(0));
        let manager = Manager::new(AccumulatorConfig::new(), {
            let close_calls = Arc::clone(&close_calls);
            move || Box::new(CountingStore::<i32>::new(Arc::clone(&close_calls), false))
        });

        manager.append("a", 1).unwrap();

        assert!(manager.close().is_ok());
        assert_eq!(close_calls.load(Ordering::SeqCst), 1);
        assert!(manager.close().is_ok());
        assert_eq!(close_calls.load(Ordering::SeqCst), 1);
    }

    #[tokio::test]
    async fn cleanup_expired_closes_all_and_surfaces_error() {
        tokio::time::pause();
        let close_calls = Arc::new(AtomicUsize::new(0));
        let manager = Manager::new(AccumulatorConfig::new().with_ttl(Duration::from_secs(5)), {
            let close_calls = Arc::clone(&close_calls);
            move || Box::new(CountingStore::<i32>::new(Arc::clone(&close_calls), true))
        });

        manager.append("a", 1).unwrap();
        manager.append("b", 2).unwrap();
        tokio::time::advance(Duration::from_secs(6)).await;

        let report = manager.cleanup_expired();
        let mut removed = report.removed.clone();
        removed.sort_unstable();
        assert_eq!(removed, vec!["a", "b"]);
        assert_eq!(close_calls.load(Ordering::SeqCst), 2);
        assert_eq!(report.error.unwrap().code(), ErrorCode::Internal);
        assert!(manager.keys().is_empty());
    }

    #[tokio::test]
    async fn cleanup_expired_succeeds_cleanly_when_closes_pass() {
        tokio::time::pause();
        let manager = Manager::new(
            AccumulatorConfig::new().with_ttl(Duration::from_secs(5)),
            || Box::new(MemoryStore::<i32>::new()),
        );

        manager.append("a", 1).unwrap();
        tokio::time::advance(Duration::from_secs(6)).await;

        assert_eq!(manager.cleanup_expired().into_result().unwrap(), vec!["a"]);
    }

    #[test]
    fn get_or_create_and_append_are_rejected_after_close() {
        let manager = Manager::new(AccumulatorConfig::new(), || {
            Box::new(MemoryStore::<i32>::new())
        });
        manager.append("a", 1).unwrap();
        manager.close().unwrap();

        assert_eq!(
            manager.get_or_create("b").err().unwrap().code(),
            ErrorCode::Conflict
        );
        assert_eq!(
            manager.append("c", 2).unwrap_err().code(),
            ErrorCode::Conflict
        );
        assert!(manager.keys().is_empty());
    }

    #[test]
    fn close_racing_get_or_create_leaves_no_unclosed_store() {
        const WORKERS: usize = 16;

        let close_calls = Arc::new(AtomicUsize::new(0));
        let manager = Arc::new(Manager::new(AccumulatorConfig::new(), {
            let close_calls = Arc::clone(&close_calls);
            move || Box::new(CountingStore::<i32>::new(Arc::clone(&close_calls), false))
        }));
        let barrier = Arc::new(Barrier::new(WORKERS + 1));

        let workers: Vec<_> = (0..WORKERS)
            .map(|worker| {
                let manager = Arc::clone(&manager);
                let barrier = Arc::clone(&barrier);
                std::thread::spawn(move || {
                    barrier.wait();
                    manager.get_or_create(worker)
                })
            })
            .collect();

        let closer = {
            let manager = Arc::clone(&manager);
            let barrier = Arc::clone(&barrier);
            std::thread::spawn(move || {
                barrier.wait();
                manager.close()
            })
        };

        let created: usize = workers
            .into_iter()
            .map(|thread| usize::from(thread.join().unwrap().is_ok()))
            .sum();
        closer.join().unwrap().unwrap();

        // Every accumulator that was successfully created is closed exactly
        // once: either drained by close() or, if inserted after the drain,
        // rejected outright (so never created). The manager ends up inert.
        assert_eq!(close_calls.load(Ordering::SeqCst), created);
        assert!(manager.keys().is_empty());
        assert_eq!(
            manager.get_or_create(999).err().unwrap().code(),
            ErrorCode::Conflict
        );
    }
}
