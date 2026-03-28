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
