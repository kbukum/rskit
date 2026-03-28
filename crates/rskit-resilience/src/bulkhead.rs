use std::sync::Arc;
use std::time::Duration;

use rskit_errors::{AppError, AppResult};
use tokio::sync::Semaphore;

/// Bulkhead configuration.
#[derive(Debug, Clone)]
pub struct BulkheadConfig {
    pub name: String,
    /// Maximum number of concurrent in-flight operations.
    pub max_concurrent: usize,
    /// How long to wait for a permit before returning `RateLimited`.
    /// `None` means wait forever.
    pub max_wait: Option<Duration>,
}

impl Default for BulkheadConfig {
    fn default() -> Self {
        Self {
            name: "bulkhead".to_string(),
            max_concurrent: 32,
            max_wait: Some(Duration::from_secs(5)),
        }
    }
}

impl BulkheadConfig {
    pub fn new(name: impl Into<String>, max_concurrent: usize) -> Self {
        Self { name: name.into(), max_concurrent, ..Default::default() }
    }

    pub fn with_max_wait(mut self, d: Duration) -> Self {
        self.max_wait = Some(d);
        self
    }

    pub fn without_wait_limit(mut self) -> Self {
        self.max_wait = None;
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
    pub fn new(config: BulkheadConfig) -> Self {
        let sem = Arc::new(Semaphore::new(config.max_concurrent));
        Self { sem, config: Arc::new(config) }
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
        let _permit = match self.config.max_wait {
            Some(timeout) => {
                tokio::time::timeout(timeout, self.sem.acquire())
                    .await
                    .map_err(|_| {
                        AppError::rate_limited()
                            .with_detail("bulkhead", self.config.name.clone())
                    })?
                    .map_err(|_| AppError::service_unavailable("bulkhead closed"))?
            }
            None => self
                .sem
                .acquire()
                .await
                .map_err(|_| AppError::service_unavailable("bulkhead closed"))?,
        };
        f().await
    }
}

#[cfg(test)]
mod tests {
    use rskit_errors::AppError;
    use super::*;

    #[tokio::test]
    async fn execute_allows_call_within_limit() {
        let bh = Bulkhead::new(BulkheadConfig::new("test", 2));
        let result = bh.execute(|| async { Ok::<i32, AppError>(1) }).await;
        assert_eq!(result.unwrap(), 1);
    }

    #[tokio::test]
    async fn available_decrements_while_executing() {
        let bh = Bulkhead::new(BulkheadConfig::new("test", 2));
        assert_eq!(bh.available(), 2);
        assert_eq!(bh.in_use(), 0);

        // After execution completes the permit is released
        let _ = bh.execute(|| async { Ok::<i32, AppError>(1) }).await;
        assert_eq!(bh.available(), 2);
    }

    #[tokio::test]
    async fn execute_allows_concurrent_calls_up_to_limit() {
        let bh = Bulkhead::new(
            BulkheadConfig::new("test", 3).with_max_wait(Duration::from_millis(100)),
        );

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
        let bh = Bulkhead::new(
            BulkheadConfig::new("test", 1)
                .with_max_wait(Duration::from_millis(10)),
        );

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
}
