//! Accumulator configuration.

use std::sync::Arc;
use std::time::Duration;

use crate::measurer::{CountMeasurer, Measurer};
use crate::trigger::Trigger;

/// Configuration for an [`crate::Accumulator`].
pub struct AccumulatorConfig<V>
where
    V: Clone,
{
    /// Optional expiration TTL for the accumulator.
    pub ttl: Option<Duration>,
    /// Whether TTL should be refreshed on append/touch.
    pub keep_alive: bool,
    /// Optional maximum measured size before oldest values are evicted.
    pub max_size: Option<usize>,
    /// Flush triggers evaluated after each append.
    pub triggers: Vec<Arc<dyn Trigger<V>>>,
    /// Value measurer used by size-based triggers and `max_size`.
    pub measurer: Arc<dyn Measurer<V>>,
}

impl<V> std::fmt::Debug for AccumulatorConfig<V>
where
    V: Clone,
{
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("AccumulatorConfig")
            .field("ttl", &self.ttl)
            .field("keep_alive", &self.keep_alive)
            .field("max_size", &self.max_size)
            .field("triggers", &self.triggers.len())
            .finish()
    }
}

impl<V> Clone for AccumulatorConfig<V>
where
    V: Clone,
{
    fn clone(&self) -> Self {
        Self {
            ttl: self.ttl,
            keep_alive: self.keep_alive,
            max_size: self.max_size,
            triggers: self.triggers.clone(),
            measurer: Arc::clone(&self.measurer),
        }
    }
}

impl<V> Default for AccumulatorConfig<V>
where
    V: Clone,
{
    fn default() -> Self {
        Self {
            ttl: None,
            keep_alive: true,
            max_size: None,
            triggers: Vec::new(),
            measurer: Arc::new(CountMeasurer),
        }
    }
}

impl<V> AccumulatorConfig<V>
where
    V: Clone,
{
    /// Create a new accumulator configuration.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Set the accumulator TTL.
    #[must_use]
    pub fn with_ttl(mut self, ttl: Duration) -> Self {
        self.ttl = Some(ttl);
        self
    }

    /// Disable TTL keep-alive refreshes.
    #[must_use]
    pub fn without_keep_alive(mut self) -> Self {
        self.keep_alive = false;
        self
    }

    /// Set the maximum measured size before oldest values are evicted.
    #[must_use]
    pub fn with_max_size(mut self, max_size: usize) -> Self {
        self.max_size = Some(max_size);
        self
    }

    /// Add a flush trigger.
    #[must_use]
    pub fn with_trigger(mut self, trigger: Arc<dyn Trigger<V>>) -> Self {
        self.triggers.push(trigger);
        self
    }

    /// Override the value measurer.
    #[must_use]
    pub fn with_measurer(mut self, measurer: Arc<dyn Measurer<V>>) -> Self {
        self.measurer = measurer;
        self
    }
}
