//! Core accumulator implementation.

use parking_lot::Mutex;
use tokio::time::Instant;

use rskit_errors::AppResult;

use crate::config::AccumulatorConfig;
use crate::store::Store;

struct AccumulatorState {
    created_at: Instant,
    last_flush: Option<Instant>,
}

/// Accumulates values until explicitly flushed or a trigger fires.
pub struct Accumulator<V>
where
    V: Clone,
{
    store: Box<dyn Store<V>>,
    config: AccumulatorConfig<V>,
    state: Mutex<AccumulatorState>,
}

impl<V> Accumulator<V>
where
    V: Clone + Send + Sync + 'static,
{
    /// Create a new accumulator.
    #[must_use]
    pub fn new(store: Box<dyn Store<V>>, config: AccumulatorConfig<V>) -> Self {
        Self {
            store,
            config,
            state: Mutex::new(AccumulatorState {
                created_at: Instant::now(),
                last_flush: None,
            }),
        }
    }

    /// Append a value and return flushed values if a trigger fires.
    pub fn append(&self, value: V) -> AppResult<Option<Vec<V>>> {
        self.store.append(value)?;
        self.enforce_max_size()?;

        if self.should_flush()? {
            let flushed = self.flush()?;
            if flushed.is_empty() {
                Ok(None)
            } else {
                Ok(Some(flushed))
            }
        } else {
            if self.config.keep_alive {
                self.store.touch()?;
            }
            Ok(None)
        }
    }

    /// Flush and return all buffered values.
    pub fn flush(&self) -> AppResult<Vec<V>> {
        let values = self.store.flush()?;
        self.state.lock().last_flush = Some(Instant::now());
        Ok(values)
    }

    /// Return the buffered item count.
    pub fn size(&self) -> AppResult<usize> {
        self.store.size()
    }

    /// Measure the buffered values using the configured measurer.
    pub fn measure(&self) -> AppResult<usize> {
        let values = self.store.snapshot()?;
        Ok(self.config.measurer.measure(&values))
    }

    /// Return whether the accumulator has expired.
    pub fn is_expired(&self) -> AppResult<bool> {
        let Some(ttl) = self.config.ttl else {
            return Ok(false);
        };
        let reference = if self.config.keep_alive {
            self.store.last_activity()?
        } else {
            self.state.lock().created_at
        };
        Ok(Instant::now().duration_since(reference) >= ttl)
    }

    /// Refresh the accumulator activity timestamp.
    pub fn touch(&self) -> AppResult<()> {
        self.store.touch()
    }

    /// Return the time elapsed since the last flush, or creation if never flushed.
    pub fn elapsed_since_flush(&self) -> std::time::Duration {
        let state = self.state.lock();
        let reference = state.last_flush.unwrap_or(state.created_at);
        Instant::now().duration_since(reference)
    }

    /// Close the underlying store.
    pub fn close(&self) -> AppResult<()> {
        self.store.close()
    }

    fn should_flush(&self) -> AppResult<bool> {
        for trigger in &self.config.triggers {
            if trigger.should_flush(self)? {
                return Ok(true);
            }
        }
        Ok(false)
    }

    fn enforce_max_size(&self) -> AppResult<()> {
        if let Some(max_size) = self.config.max_size {
            while self.measure()? > max_size {
                if self.store.pop_oldest()?.is_none() {
                    break;
                }
            }
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;
    use std::time::Duration;

    use super::*;
    use crate::{AccumulatorConfig, CountMeasurer, MemoryStore, SizeTrigger, TimeTrigger};

    #[test]
    fn append_flushes_when_size_trigger_fires() {
        let accumulator = Accumulator::new(
            Box::new(MemoryStore::new()),
            AccumulatorConfig::new().with_trigger(Arc::new(SizeTrigger::new(2))),
        );

        assert!(accumulator.append(1).unwrap().is_none());
        assert_eq!(accumulator.append(2).unwrap(), Some(vec![1, 2]));
    }

    #[tokio::test]
    async fn ttl_expiration_respects_keep_alive_setting() {
        tokio::time::pause();
        let accumulator = Accumulator::new(
            Box::new(MemoryStore::new()),
            AccumulatorConfig::new().with_ttl(Duration::from_secs(5)),
        );

        accumulator.append(1).unwrap();
        tokio::time::advance(Duration::from_secs(6)).await;
        assert!(accumulator.is_expired().unwrap());

        let absolute = Accumulator::new(
            Box::new(MemoryStore::new()),
            AccumulatorConfig::new()
                .with_ttl(Duration::from_secs(5))
                .without_keep_alive()
                .with_measurer(Arc::new(CountMeasurer)),
        );
        absolute.append(1).unwrap();
        tokio::time::advance(Duration::from_secs(6)).await;
        assert!(absolute.is_expired().unwrap());
    }

    #[tokio::test]
    async fn time_trigger_flushes_after_interval() {
        tokio::time::pause();
        let accumulator = Accumulator::new(
            Box::new(MemoryStore::new()),
            AccumulatorConfig::new()
                .with_trigger(Arc::new(TimeTrigger::new(Duration::from_secs(5)))),
        );

        accumulator.append(1).unwrap();
        tokio::time::advance(Duration::from_secs(6)).await;
        assert_eq!(accumulator.append(2).unwrap(), Some(vec![1, 2]));
    }

    #[test]
    fn max_size_eviction_and_empty_flush_paths_are_reported() {
        let accumulator = Accumulator::new(
            Box::new(MemoryStore::new()),
            AccumulatorConfig::new().with_max_size(1),
        );

        assert!(!accumulator.is_expired().unwrap());
        assert_eq!(accumulator.append(1).unwrap(), None);
        assert_eq!(accumulator.append(2).unwrap(), None);
        assert_eq!(accumulator.flush().unwrap(), vec![2]);

        let flushing = Accumulator::new(
            Box::new(MemoryStore::<i32>::new()),
            AccumulatorConfig::new().with_trigger(Arc::new(SizeTrigger::new(0))),
        );
        assert_eq!(flushing.append(1).unwrap(), Some(vec![1]));
        flushing.touch().unwrap();
        flushing.close().unwrap();
    }
}
