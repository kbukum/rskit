//! Flush trigger implementations.

use std::time::Duration;

use rskit_errors::AppResult;

use crate::Accumulator;

/// Decides when an accumulator should flush.
pub trait Trigger<V>: Send + Sync
where
    V: Clone + Send + Sync + 'static,
{
    /// Return `true` when the accumulator should flush.
    fn should_flush(&self, accumulator: &Accumulator<V>) -> AppResult<bool>;

    /// Human-readable trigger name.
    fn name(&self) -> &'static str;
}

/// Flush when buffered item count reaches `threshold`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SizeTrigger {
    /// Threshold that triggers a flush.
    pub threshold: usize,
}

impl SizeTrigger {
    /// Create a new size trigger.
    #[must_use]
    pub fn new(threshold: usize) -> Self {
        Self { threshold }
    }
}

impl<V> Trigger<V> for SizeTrigger
where
    V: Clone + Send + Sync + 'static,
{
    fn should_flush(&self, accumulator: &Accumulator<V>) -> AppResult<bool> {
        Ok(accumulator.size()? >= self.threshold)
    }

    fn name(&self) -> &'static str {
        "size"
    }
}

/// Flush when buffered byte size reaches `threshold`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ByteSizeTrigger {
    /// Threshold that triggers a flush.
    pub threshold: usize,
}

impl ByteSizeTrigger {
    /// Create a new byte-size trigger.
    #[must_use]
    pub fn new(threshold: usize) -> Self {
        Self { threshold }
    }
}

impl<V> Trigger<V> for ByteSizeTrigger
where
    V: Clone + Send + Sync + 'static,
{
    fn should_flush(&self, accumulator: &Accumulator<V>) -> AppResult<bool> {
        Ok(accumulator.measure()? >= self.threshold)
    }

    fn name(&self) -> &'static str {
        "byte_size"
    }
}

/// Flush when the time since the last flush exceeds `interval`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TimeTrigger {
    /// Interval that triggers a flush.
    pub interval: Duration,
}

impl TimeTrigger {
    /// Create a new time trigger.
    #[must_use]
    pub fn new(interval: Duration) -> Self {
        Self { interval }
    }
}

impl<V> Trigger<V> for TimeTrigger
where
    V: Clone + Send + Sync + 'static,
{
    fn should_flush(&self, accumulator: &Accumulator<V>) -> AppResult<bool> {
        Ok(accumulator.elapsed_since_flush() >= self.interval)
    }

    fn name(&self) -> &'static str {
        "time"
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use super::*;
    use crate::{Accumulator, AccumulatorConfig, ByteSizeMeasurer, MemoryStore};

    #[test]
    fn byte_size_trigger_uses_configured_measurer_and_names_are_stable() {
        let trigger = ByteSizeTrigger::new(3);
        let accumulator = Accumulator::new(
            Box::new(MemoryStore::new()),
            AccumulatorConfig::new().with_measurer(Arc::new(ByteSizeMeasurer)),
        );

        accumulator.append(b"ab".to_vec()).unwrap();

        assert_eq!(
            <SizeTrigger as Trigger<Vec<u8>>>::name(&SizeTrigger::new(1)),
            "size"
        );
        assert_eq!(
            <ByteSizeTrigger as Trigger<Vec<u8>>>::name(&trigger),
            "byte_size"
        );
        assert_eq!(
            <TimeTrigger as Trigger<Vec<u8>>>::name(&TimeTrigger::new(Duration::from_secs(1))),
            "time"
        );
        assert!(!trigger.should_flush(&accumulator).unwrap());
        accumulator.append(b"cd".to_vec()).unwrap();
        assert!(trigger.should_flush(&accumulator).unwrap());
    }
}
