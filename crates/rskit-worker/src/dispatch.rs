use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};

/// Strategy used to pick a worker slot in the pool.
#[derive(Debug, Clone, Default)]
pub enum DispatchStrategy {
    /// Assign tasks round-robin across available worker slots.
    #[default]
    RoundRobin,
    /// Prefer slots with the lowest active task count (future extension).
    LeastLoaded,
}

/// Simple round-robin counter shared across the pool.
#[derive(Clone)]
pub struct RoundRobinDispatcher {
    counter: Arc<AtomicUsize>,
    slots: usize,
}

impl RoundRobinDispatcher {
    /// Create a new dispatcher that distributes work across `slots` worker slots.
    pub fn new(slots: usize) -> Self {
        Self {
            counter: Arc::new(AtomicUsize::new(0)),
            slots,
        }
    }

    /// Return the next worker index.
    pub fn next(&self) -> usize {
        if self.slots == 0 {
            return 0;
        }
        self.counter.fetch_add(1, Ordering::Relaxed) % self.slots
    }
}
