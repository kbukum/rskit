//! Component test doubles.

use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};

use rskit_component::{Component, Health};
use rskit_errors::AppResult;

/// Deterministic component fake for lifecycle tests.
pub struct FakeComponent {
    name: String,
    started: AtomicBool,
    stopped: AtomicBool,
    start_count: AtomicUsize,
    stop_count: AtomicUsize,
}

impl FakeComponent {
    /// Create a fake component with the given name.
    #[must_use]
    pub fn new(name: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            started: AtomicBool::new(false),
            stopped: AtomicBool::new(false),
            start_count: AtomicUsize::new(0),
            stop_count: AtomicUsize::new(0),
        }
    }

    /// Number of successful start calls.
    #[must_use]
    pub fn start_count(&self) -> usize {
        self.start_count.load(Ordering::SeqCst)
    }

    /// Number of successful stop calls.
    #[must_use]
    pub fn stop_count(&self) -> usize {
        self.stop_count.load(Ordering::SeqCst)
    }
}

#[async_trait::async_trait]
impl Component for FakeComponent {
    fn name(&self) -> &str {
        &self.name
    }

    async fn start(&self) -> AppResult<()> {
        self.started.store(true, Ordering::SeqCst);
        self.stopped.store(false, Ordering::SeqCst);
        self.start_count.fetch_add(1, Ordering::SeqCst);
        Ok(())
    }

    async fn stop(&self) -> AppResult<()> {
        self.stopped.store(true, Ordering::SeqCst);
        self.stop_count.fetch_add(1, Ordering::SeqCst);
        Ok(())
    }

    fn health(&self) -> Health {
        if self.started.load(Ordering::SeqCst) && !self.stopped.load(Ordering::SeqCst) {
            Health::healthy(&self.name)
        } else {
            Health::unhealthy(&self.name, "not running")
        }
    }
}
