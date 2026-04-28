//! Periodic ticker worker that implements [`rskit_bootstrap::Component`].
//!
//! [`TickerWorker`] runs a user-supplied async function on a fixed interval
//! in a background Tokio task. It integrates with the bootstrap lifecycle
//! via the [`rskit_bootstrap::Component`] trait.
//!
//! # Example
//!
//! ```rust,no_run
//! use rskit_worker::TickerWorker;
//! use std::time::Duration;
//!
//! let ticker = TickerWorker::new("cache-cleanup", Duration::from_secs(30), || {
//!     Box::pin(async {
//!         // cleanup logic here
//!         Ok(())
//!     })
//! });
//! // Register with bootstrap registry, then start via registry.start_all()
//! ```

use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::time::Duration;

use parking_lot::RwLock;
use rskit_bootstrap::{Component, Health};
use rskit_errors::AppResult;
use tokio::time;
use tokio_util::sync::CancellationToken;

/// A future returned by the tick handler.
pub type TickFuture = std::pin::Pin<Box<dyn std::future::Future<Output = AppResult<()>> + Send>>;

/// Shared state between the component and the background task.
struct Inner {
    running: AtomicBool,
    run_count: AtomicU64,
    fail_count: AtomicU64,
    last_error: RwLock<Option<String>>,
}

/// A component that runs a function on a fixed interval.
///
/// Implements [`Component`] for integration with the bootstrap lifecycle.
/// Start launches a background Tokio task; stop cancels it gracefully.
///
/// # Example
///
/// ```rust,no_run
/// use rskit_worker::TickerWorker;
/// use std::time::Duration;
///
/// let ticker = TickerWorker::new("cache-cleanup", Duration::from_secs(30), || {
///     Box::pin(async { Ok(()) })
/// });
/// ```
pub struct TickerWorker {
    name: String,
    interval: Duration,
    handler: Arc<dyn Fn() -> TickFuture + Send + Sync>,
    cancel: CancellationToken,
    inner: Arc<Inner>,
}

impl TickerWorker {
    /// Create a new ticker worker.
    ///
    /// # Arguments
    ///
    /// * `name`     – component name for logging and health.
    /// * `interval` – time between ticks.
    /// * `handler`  – factory returning a future to execute on each tick.
    pub fn new<F>(name: impl Into<String>, interval: Duration, handler: F) -> Self
    where
        F: Fn() -> TickFuture + Send + Sync + 'static,
    {
        Self {
            name: name.into(),
            interval,
            handler: Arc::new(handler),
            cancel: CancellationToken::new(),
            inner: Arc::new(Inner {
                running: AtomicBool::new(false),
                run_count: AtomicU64::new(0),
                fail_count: AtomicU64::new(0),
                last_error: RwLock::new(None),
            }),
        }
    }

    /// Total number of completed ticks.
    pub fn run_count(&self) -> u64 {
        self.inner.run_count.load(Ordering::Relaxed)
    }

    /// Total number of failed ticks.
    pub fn fail_count(&self) -> u64 {
        self.inner.fail_count.load(Ordering::Relaxed)
    }
}

#[async_trait::async_trait]
impl Component for TickerWorker {
    fn name(&self) -> &str {
        &self.name
    }

    async fn start(&self) -> AppResult<()> {
        if self.inner.running.swap(true, Ordering::SeqCst) {
            return Ok(());
        }

        let cancel = self.cancel.clone();
        let interval = self.interval;
        let handler = self.handler.clone();
        let inner = self.inner.clone();

        tokio::spawn(async move {
            let mut tick = time::interval(interval);
            tick.tick().await; // skip immediate first tick

            loop {
                tokio::select! {
                    _ = cancel.cancelled() => break,
                    _ = tick.tick() => {
                        let result = (handler)().await;
                        inner.run_count.fetch_add(1, Ordering::Relaxed);
                        match result {
                            Ok(()) => {
                                *inner.last_error.write() = None;
                            }
                            Err(e) => {
                                inner.fail_count.fetch_add(1, Ordering::Relaxed);
                                *inner.last_error.write() = Some(e.to_string());
                            }
                        }
                    }
                }
            }
        });

        Ok(())
    }

    async fn stop(&self) -> AppResult<()> {
        if !self.inner.running.swap(false, Ordering::SeqCst) {
            return Ok(());
        }
        self.cancel.cancel();
        Ok(())
    }

    fn health(&self) -> Health {
        if !self.inner.running.load(Ordering::Relaxed) {
            return Health::unhealthy(&self.name, "not running");
        }
        let last_err = self.inner.last_error.read();
        if let Some(err) = last_err.as_ref() {
            return Health::degraded(&self.name, err.clone());
        }
        Health::healthy(&self.name)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rskit_bootstrap::HealthStatus;
    use rskit_errors::{AppError, ErrorCode};
    use std::sync::atomic::AtomicI32;

    #[tokio::test]
    async fn runs_on_interval() {
        let count = Arc::new(AtomicI32::new(0));
        let count2 = count.clone();

        let tw = TickerWorker::new("test", Duration::from_millis(20), move || {
            count2.fetch_add(1, Ordering::Relaxed);
            Box::pin(async { Ok(()) })
        });

        tw.start().await.unwrap();
        tokio::time::sleep(Duration::from_millis(75)).await;
        tw.stop().await.unwrap();

        assert!(count.load(Ordering::Relaxed) >= 2, "expected ≥2 ticks");
    }

    #[tokio::test]
    async fn health_before_start_is_unhealthy() {
        let tw = TickerWorker::new("h", Duration::from_secs(1), || Box::pin(async { Ok(()) }));
        assert_eq!(tw.health().status, HealthStatus::Unhealthy);
    }

    #[tokio::test]
    async fn health_after_start_is_healthy() {
        let tw = TickerWorker::new("h", Duration::from_millis(10), || {
            Box::pin(async { Ok(()) })
        });
        tw.start().await.unwrap();
        tokio::time::sleep(Duration::from_millis(25)).await;
        assert_eq!(tw.health().status, HealthStatus::Healthy);
        tw.stop().await.unwrap();
    }

    #[tokio::test]
    async fn health_degraded_on_error() {
        let tw = TickerWorker::new("err", Duration::from_millis(10), || {
            Box::pin(async { Err(AppError::new(ErrorCode::Internal, "boom")) })
        });
        tw.start().await.unwrap();
        tokio::time::sleep(Duration::from_millis(25)).await;
        let h = tw.health();
        assert_eq!(h.status, HealthStatus::Degraded);
        tw.stop().await.unwrap();
    }

    #[tokio::test]
    async fn stop_is_idempotent() {
        let tw = TickerWorker::new("idem", Duration::from_secs(1), || {
            Box::pin(async { Ok(()) })
        });
        tw.stop().await.unwrap(); // before start
        tw.start().await.unwrap();
        tw.stop().await.unwrap();
        tw.stop().await.unwrap(); // double stop
    }

    #[test]
    fn name_accessor() {
        let tw = TickerWorker::new("my-worker", Duration::from_secs(1), || {
            Box::pin(async { Ok(()) })
        });
        assert_eq!(tw.name(), "my-worker");
    }

    #[tokio::test]
    async fn run_count_tracks() {
        let tw = TickerWorker::new("cnt", Duration::from_millis(10), || {
            Box::pin(async { Ok(()) })
        });
        tw.start().await.unwrap();
        tokio::time::sleep(Duration::from_millis(80)).await;
        tw.stop().await.unwrap();
        assert!(tw.run_count() >= 2);
    }

    #[tokio::test]
    async fn fail_count_tracks() {
        let tw = TickerWorker::new("fail", Duration::from_millis(10), || {
            Box::pin(async { Err(AppError::new(ErrorCode::Internal, "test error")) })
        });
        tw.start().await.unwrap();
        tokio::time::sleep(Duration::from_millis(80)).await;
        tw.stop().await.unwrap();
        assert!(tw.fail_count() >= 2);
    }
}
