//! Retry middleware backed by [`rskit_resilience::RetryPolicy`].

use std::sync::Arc;

use async_trait::async_trait;
use rskit_errors::AppResult;
use rskit_resilience::{ConstantBackoff, LinearBackoff, RetryError, RetryPolicy};

use crate::handler::{HandlerMiddleware, MessageHandler};
use crate::message::Message;

/// Canonical retry configuration for messaging middleware.
///
/// Messaging retries historically retried every handler failure by default.
/// `RetryPolicy` defaults to retrying only errors marked retryable,
/// so this wrapper encodes the messaging-specific always-retry default explicitly.
#[derive(Debug, Clone)]
pub struct RetryConfig {
    policy: RetryPolicy,
}

impl Default for RetryConfig {
    fn default() -> Self {
        Self {
            policy: RetryPolicy::new().with_retry_if(|_| true),
        }
    }
}

impl RetryConfig {
    /// Create a retry config with messaging defaults.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Wrap an explicit resilience retry policy.
    #[must_use]
    pub const fn from_policy(policy: RetryPolicy) -> Self {
        Self { policy }
    }

    /// Set the maximum number of attempts (including the first call).
    #[must_use]
    pub fn with_max_attempts(mut self, n: usize) -> Self {
        self.policy = self.policy.with_max_attempts(n);
        self
    }

    /// Set the initial backoff delay before the first retry.
    #[must_use]
    pub fn with_initial_backoff(mut self, d: std::time::Duration) -> Self {
        self.policy = self.policy.with_initial_backoff(d);
        self
    }

    /// Set the upper bound on any single backoff delay.
    #[must_use]
    pub fn with_max_backoff(mut self, d: std::time::Duration) -> Self {
        self.policy = self.policy.with_max_backoff(d);
        self
    }

    /// Set the exponential backoff multiplication factor.
    #[must_use]
    pub fn with_backoff_factor(mut self, factor: f64) -> Self {
        self.policy = self.policy.with_backoff_factor(factor);
        self
    }

    /// Use a fixed delay for every retry attempt.
    #[must_use]
    pub fn with_constant_backoff(mut self, backoff: ConstantBackoff) -> Self {
        self.policy = self.policy.with_constant_backoff(backoff);
        self
    }

    /// Use a linearly increasing retry delay.
    #[must_use]
    pub fn with_linear_backoff(mut self, backoff: LinearBackoff) -> Self {
        self.policy = self.policy.with_linear_backoff(backoff);
        self
    }

    /// Enable or disable retry jitter.
    #[must_use]
    pub fn with_jitter(mut self, enabled: bool) -> Self {
        self.policy = self.policy.with_jitter(enabled);
        self
    }

    /// Override the predicate used to decide whether an error is retried.
    #[must_use]
    pub fn with_retry_if(
        mut self,
        f: impl Fn(&rskit_errors::AppError) -> bool + Send + Sync + 'static,
    ) -> Self {
        self.policy = self.policy.with_retry_if(f);
        self
    }

    /// Register a callback called after each failed attempt before the next backoff sleep.
    #[must_use]
    pub fn with_on_retry(
        mut self,
        f: impl Fn(u32, &rskit_errors::AppError) + Send + Sync + 'static,
    ) -> Self {
        self.policy = self.policy.with_on_retry(f);
        self
    }

    /// Borrow the underlying resilience policy.
    #[must_use]
    pub const fn policy(&self) -> &RetryPolicy {
        &self.policy
    }

    /// Convert into the underlying resilience policy.
    #[must_use]
    pub fn into_policy(self) -> RetryPolicy {
        self.policy
    }

    async fn execute<F, Fut, T>(&self, f: F) -> Result<T, RetryError>
    where
        F: FnMut() -> Fut,
        Fut: std::future::Future<Output = AppResult<T>>,
    {
        self.policy.execute(f).await
    }
}

impl From<RetryPolicy> for RetryConfig {
    fn from(policy: RetryPolicy) -> Self {
        Self::from_policy(policy)
    }
}

/// Create a retry middleware with the given canonical retry policy.
pub fn retry<T: Send + Sync + Clone + 'static>(config: RetryConfig) -> impl HandlerMiddleware<T> {
    RetryMiddleware { config }
}

struct RetryMiddleware {
    config: RetryConfig,
}

impl<T: Send + Sync + Clone + 'static> HandlerMiddleware<T> for RetryMiddleware {
    fn wrap(&self, next: Arc<dyn MessageHandler<T>>) -> Arc<dyn MessageHandler<T>> {
        Arc::new(RetryHandler {
            config: self.config.clone(),
            next,
        })
    }
}

struct RetryHandler<T: Send + Sync + 'static> {
    config: RetryConfig,
    next: Arc<dyn MessageHandler<T>>,
}

#[async_trait]
impl<T: Send + Sync + Clone + 'static> MessageHandler<T> for RetryHandler<T> {
    async fn handle(&self, msg: Message<T>) -> AppResult<()> {
        let next = self.next.clone();
        self.config
            .execute(|| {
                let next = next.clone();
                let msg = msg.clone();
                async move { next.handle(msg).await }
            })
            .await
            .map_err(|err| err.last_error)
    }
}

#[cfg(test)]
mod tests {
    use std::sync::atomic::{AtomicU32, Ordering};
    use std::time::Duration;

    use super::*;
    use crate::handler::{FnHandler, chain_handlers};
    use rskit_errors::{AppError, ErrorCode};

    fn test_policy() -> RetryConfig {
        RetryConfig::new()
            .with_max_attempts(3)
            .with_initial_backoff(Duration::from_millis(1))
            .with_jitter(false)
    }

    #[tokio::test]
    async fn retry_success_on_first_attempt() {
        let counter = Arc::new(AtomicU32::new(0));
        let c = counter.clone();
        let base: Arc<dyn MessageHandler<String>> =
            Arc::new(FnHandler::new(move |_msg: Message<String>| {
                let c = c.clone();
                async move {
                    c.fetch_add(1, Ordering::SeqCst);
                    Ok(())
                }
            }));

        let handler = chain_handlers(
            base,
            &[Arc::new(RetryMiddleware {
                config: test_policy(),
            })],
        );

        handler
            .handle(Message::new("t", "data".to_string()))
            .await
            .unwrap();
        assert_eq!(counter.load(Ordering::SeqCst), 1);
    }

    #[tokio::test]
    async fn retry_succeeds_on_second_attempt() {
        let counter = Arc::new(AtomicU32::new(0));
        let c = counter.clone();
        let base: Arc<dyn MessageHandler<String>> =
            Arc::new(FnHandler::new(move |_msg: Message<String>| {
                let c = c.clone();
                async move {
                    let n = c.fetch_add(1, Ordering::SeqCst);
                    if n == 0 {
                        Err(AppError::new(ErrorCode::Internal, "transient"))
                    } else {
                        Ok(())
                    }
                }
            }));

        let handler = chain_handlers(
            base,
            &[Arc::new(RetryMiddleware {
                config: test_policy(),
            })],
        );

        handler
            .handle(Message::new("t", "data".to_string()))
            .await
            .unwrap();
        assert_eq!(counter.load(Ordering::SeqCst), 2);
    }

    #[tokio::test]
    async fn retry_exhausts_all_attempts() {
        let counter = Arc::new(AtomicU32::new(0));
        let c = counter.clone();
        let base: Arc<dyn MessageHandler<String>> =
            Arc::new(FnHandler::new(move |_msg: Message<String>| {
                let c = c.clone();
                async move {
                    c.fetch_add(1, Ordering::SeqCst);
                    Err(AppError::new(ErrorCode::Internal, "always fails"))
                }
            }));

        let handler = chain_handlers(
            base,
            &[Arc::new(RetryMiddleware {
                config: test_policy(),
            })],
        );

        let result = handler.handle(Message::new("t", "data".to_string())).await;
        assert!(result.is_err());
        assert_eq!(counter.load(Ordering::SeqCst), 3);
    }
    #[tokio::test]
    async fn default_retry_config_retries_non_retryable_handler_errors() {
        let counter = Arc::new(AtomicU32::new(0));
        let c = counter.clone();
        let base: Arc<dyn MessageHandler<String>> =
            Arc::new(FnHandler::new(move |_msg: Message<String>| {
                let c = c.clone();
                async move {
                    c.fetch_add(1, Ordering::SeqCst);
                    Err(AppError::new(
                        ErrorCode::InvalidInput,
                        "invalid but retried",
                    ))
                }
            }));

        let handler = chain_handlers(
            base,
            &[Arc::new(RetryMiddleware {
                config: RetryConfig::new()
                    .with_max_attempts(2)
                    .with_initial_backoff(Duration::from_millis(1))
                    .with_jitter(false),
            })],
        );

        let result = handler.handle(Message::new("t", "data".to_string())).await;

        assert!(result.is_err());
        assert_eq!(counter.load(Ordering::SeqCst), 2);
    }

    #[tokio::test]
    async fn public_retry_constructor_and_config_builders_wrap_handler() {
        let callback_count = Arc::new(AtomicU32::new(0));
        let callback_seen = callback_count.clone();
        let config = RetryConfig::from_policy(RetryPolicy::new())
            .with_max_attempts(1)
            .with_max_backoff(Duration::from_millis(5))
            .with_backoff_factor(1.0)
            .with_constant_backoff(ConstantBackoff::new(Duration::from_millis(1)))
            .with_linear_backoff(LinearBackoff::new(
                Duration::from_millis(1),
                Duration::from_millis(1),
                Duration::from_millis(5),
            ))
            .with_retry_if(|_| false)
            .with_on_retry(move |_, _| {
                callback_seen.fetch_add(1, Ordering::SeqCst);
            });
        let _policy = config.clone().into_policy();
        let _borrowed = config.policy();

        let base: Arc<dyn MessageHandler<String>> =
            Arc::new(FnHandler::new(|_msg: Message<String>| async { Ok(()) }));
        let middleware: Arc<dyn HandlerMiddleware<String>> = Arc::new(retry(config));
        let handler = chain_handlers(base, &[middleware]);

        handler
            .handle(Message::new("t", "ok".to_string()))
            .await
            .unwrap();
        assert_eq!(callback_count.load(Ordering::SeqCst), 0);
    }
}
