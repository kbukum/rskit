//! Shared test doubles for stateful accumulator and manager tests.

use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};

use parking_lot::Mutex;
use tokio::time::Instant;

use rskit_errors::{AppError, AppResult, ErrorCode};

use crate::Store;

/// In-memory [`Store`] that records how many times [`Store::close`] is invoked and can be scripted to fail on close, so teardown-error paths can be exercised deterministically. The close counter is shared via an [`Arc<AtomicUsize>`] handed in at construction, so a manager factory can aggregate the close count across every store it creates.
pub struct CountingStore<V> {
    items: Mutex<Vec<V>>,
    last_activity: Mutex<Instant>,
    close_calls: Arc<AtomicUsize>,
    fail_on_close: bool,
}

impl<V> CountingStore<V> {
    /// Create a store that increments `close_calls` on each `close`, failing with a sentinel [`ErrorCode::Internal`] error when `fail_on_close`.
    #[must_use]
    pub fn new(close_calls: Arc<AtomicUsize>, fail_on_close: bool) -> Self {
        Self {
            items: Mutex::new(Vec::new()),
            last_activity: Mutex::new(Instant::now()),
            close_calls,
            fail_on_close,
        }
    }
}

impl<V> Store<V> for CountingStore<V>
where
    V: Clone + Send + Sync,
{
    fn append(&self, value: V) -> AppResult<()> {
        self.items.lock().push(value);
        Ok(())
    }

    fn pop_oldest(&self) -> AppResult<Option<V>> {
        let mut items = self.items.lock();
        if items.is_empty() {
            Ok(None)
        } else {
            Ok(Some(items.remove(0)))
        }
    }

    fn snapshot(&self) -> AppResult<Vec<V>> {
        Ok(self.items.lock().clone())
    }

    fn flush(&self) -> AppResult<Vec<V>> {
        Ok(std::mem::take(&mut *self.items.lock()))
    }

    fn size(&self) -> AppResult<usize> {
        Ok(self.items.lock().len())
    }

    fn touch(&self) -> AppResult<()> {
        *self.last_activity.lock() = Instant::now();
        Ok(())
    }

    fn last_activity(&self) -> AppResult<Instant> {
        Ok(*self.last_activity.lock())
    }

    fn close(&self) -> AppResult<()> {
        self.close_calls.fetch_add(1, Ordering::SeqCst);
        if self.fail_on_close {
            Err(AppError::new(
                ErrorCode::Internal,
                "counting store close failed",
            ))
        } else {
            Ok(())
        }
    }
}
