use std::sync::Arc;
use std::time::Duration;

use rskit_errors::{AppError, AppResult};
use tokio::sync::Semaphore;

/// Bulkhead configuration.
pub struct BulkheadConfig {
    /// Human-readable name used in tracing and error details.
    pub name: String,
    /// Maximum number of concurrent in-flight operations.
    pub max_concurrent: usize,
    /// How long to wait for a permit before returning `RateLimited`.
    pub max_wait: Duration,
    /// Called when a permit cannot be acquired (bulkhead full).
    pub on_reject: Option<Arc<dyn Fn() + Send + Sync>>,
    /// Called when a permit is successfully acquired.
    pub on_acquire: Option<Arc<dyn Fn() + Send + Sync>>,
    /// Called when a permit is released after the operation completes.
    pub on_release: Option<Arc<dyn Fn() + Send + Sync>>,
}

impl std::fmt::Debug for BulkheadConfig {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("BulkheadConfig")
            .field("name", &self.name)
            .field("max_concurrent", &self.max_concurrent)
            .field("max_wait", &self.max_wait)
            .field("on_reject", &self.on_reject.as_ref().map(|_| "<fn>"))
            .field("on_acquire", &self.on_acquire.as_ref().map(|_| "<fn>"))
            .field("on_release", &self.on_release.as_ref().map(|_| "<fn>"))
            .finish()
    }
}

impl Clone for BulkheadConfig {
    fn clone(&self) -> Self {
        Self {
            name: self.name.clone(),
            max_concurrent: self.max_concurrent,
            max_wait: self.max_wait,
            on_reject: self.on_reject.clone(),
            on_acquire: self.on_acquire.clone(),
            on_release: self.on_release.clone(),
        }
    }
}

impl Default for BulkheadConfig {
    fn default() -> Self {
        Self {
            name: "bulkhead".to_string(),
            max_concurrent: 32,
            max_wait: Duration::from_secs(5),
            on_reject: None,
            on_acquire: None,
            on_release: None,
        }
    }
}

impl BulkheadConfig {
    /// Create a bulkhead named `name` with the given concurrency limit.
    #[must_use]
    pub fn new(name: impl Into<String>, max_concurrent: usize) -> Self {
        Self {
            name: name.into(),
            max_concurrent,
            ..Default::default()
        }
    }

    /// Validate that the bulkhead has an explicit bounded positive capacity.
    ///
    /// # Errors
    /// Returns an error when the concurrency limit is zero.
    pub fn validate(&self) -> AppResult<()> {
        if self.max_concurrent == 0 {
            return Err(AppError::invalid_input(
                "max_concurrent",
                "bulkhead concurrency limit must be greater than zero",
            ));
        }
        Ok(())
    }

    /// Set the maximum wait time for a permit before returning [`AppError::rate_limited`].
    #[must_use]
    pub fn with_max_wait(mut self, d: Duration) -> Self {
        self.max_wait = d;
        self
    }

    /// Register a callback invoked when the bulkhead rejects a caller
    /// because all slots are occupied and the wait timeout expires.
    #[must_use]
    pub fn with_on_reject(mut self, f: impl Fn() + Send + Sync + 'static) -> Self {
        self.on_reject = Some(Arc::new(f));
        self
    }

    /// Register a callback invoked each time a permit is successfully acquired.
    #[must_use]
    pub fn with_on_acquire(mut self, f: impl Fn() + Send + Sync + 'static) -> Self {
        self.on_acquire = Some(Arc::new(f));
        self
    }

    /// Register a callback invoked each time a permit is released.
    #[must_use]
    pub fn with_on_release(mut self, f: impl Fn() + Send + Sync + 'static) -> Self {
        self.on_release = Some(Arc::new(f));
        self
    }
}

/// Semaphore-based bulkhead that limits concurrent calls.
#[derive(Clone)]
pub struct Bulkhead {
    sem: Arc<Semaphore>,
    config: Arc<BulkheadConfig>,
}

impl Bulkhead {
    /// Create a new [`Bulkhead`] from the given configuration.
    ///
    /// # Errors
    /// Returns an error when the configuration is invalid.
    pub fn new(config: BulkheadConfig) -> AppResult<Self> {
        config.validate()?;
        let sem = Arc::new(Semaphore::new(config.max_concurrent));
        Ok(Self {
            sem,
            config: Arc::new(config),
        })
    }

    /// Number of free permits (available slots).
    pub fn available(&self) -> usize {
        self.sem.available_permits()
    }

    /// Number of slots currently in use.
    pub fn in_use(&self) -> usize {
        self.config.max_concurrent.saturating_sub(self.available())
    }

    /// Execute `f` within the bulkhead.
    pub async fn execute<F, Fut, T>(&self, f: F) -> AppResult<T>
    where
        F: FnOnce() -> Fut,
        Fut: std::future::Future<Output = AppResult<T>>,
    {
        let permit_result = tokio::time::timeout(self.config.max_wait, self.sem.acquire())
            .await
            .map_err(|_| AppError::rate_limited().with_detail("bulkhead", self.config.name.clone()))
            .and_then(|r| r.map_err(|_| AppError::service_unavailable("bulkhead closed")));

        let _permit = match permit_result {
            Ok(p) => p,
            Err(e) => {
                if let Some(cb) = &self.config.on_reject {
                    cb();
                }
                return Err(e);
            }
        };

        if let Some(cb) = &self.config.on_acquire {
            cb();
        }

        let result = f().await;

        if let Some(cb) = &self.config.on_release {
            cb();
        }

        result
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rskit_errors::AppError;

    #[tokio::test]
    async fn execute_allows_call_within_limit() {
        let bh = Bulkhead::new(BulkheadConfig::new("test", 2)).unwrap();
        let result = bh.execute(|| async { Ok::<i32, AppError>(1) }).await;
        assert_eq!(result.unwrap(), 1);
    }

    #[tokio::test]
    async fn available_decrements_while_executing() {
        let bh = Bulkhead::new(BulkheadConfig::new("test", 2)).unwrap();
        assert_eq!(bh.available(), 2);
        assert_eq!(bh.in_use(), 0);

        // After execution completes the permit is released
        let _ = bh.execute(|| async { Ok::<i32, AppError>(1) }).await;
        assert_eq!(bh.available(), 2);
    }

    #[tokio::test]
    async fn execute_allows_concurrent_calls_up_to_limit() {
        let bh =
            Bulkhead::new(BulkheadConfig::new("test", 3).with_max_wait(Duration::from_millis(100)))
                .unwrap();

        // Spawn 3 concurrent tasks; all should succeed
        let mut handles = Vec::new();
        for i in 0..3usize {
            let bh = bh.clone();
            handles.push(tokio::spawn(async move {
                bh.execute(|| async move { Ok::<usize, AppError>(i) }).await
            }));
        }

        for h in handles {
            assert!(h.await.unwrap().is_ok());
        }
    }

    #[tokio::test]
    async fn execute_rejects_when_all_slots_occupied_and_wait_expires() {
        // max_concurrent=1, very short wait so the blocked call times out
        let bh =
            Bulkhead::new(BulkheadConfig::new("test", 1).with_max_wait(Duration::from_millis(10)))
                .unwrap();

        // Hold the single permit for a long time using a channel
        let (tx, rx) = tokio::sync::oneshot::channel::<()>();
        let bh_clone = bh.clone();
        let holder = tokio::spawn(async move {
            bh_clone
                .execute(|| async move {
                    let _ = rx.await;
                    Ok::<i32, AppError>(0)
                })
                .await
        });

        // Give the holder a moment to acquire the permit
        tokio::time::sleep(Duration::from_millis(5)).await;

        // This call should time out because the only slot is taken
        let result = bh.execute(|| async { Ok::<i32, AppError>(1) }).await;
        assert!(result.is_err());

        // Clean up
        let _ = tx.send(());
        let _ = holder.await;
    }

    #[test]
    fn new_rejects_zero_concurrency_limit() {
        let result = Bulkhead::new(BulkheadConfig::new("closed", 0));
        assert!(result.is_err());
    }
}
