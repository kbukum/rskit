//! Retry middleware with exponential backoff.

use std::sync::Arc;
use std::time::Duration;

use async_trait::async_trait;
use rskit_errors::AppResult;

use crate::handler::{HandlerMiddleware, MessageHandler};
use crate::message::Message;

/// Configuration for the retry middleware.
#[derive(Debug, Clone)]
pub struct RetryConfig {
    /// Maximum number of attempts (including the first call).
    pub max_attempts: u32,
    /// Delay before the first retry.
    pub initial_backoff: Duration,
    /// Upper bound on any single backoff delay.
    pub max_backoff: Duration,
    /// Multiplier applied on each successive retry.
    pub backoff_factor: f64,
}

impl Default for RetryConfig {
    fn default() -> Self {
        Self {
            max_attempts: 3,
            initial_backoff: Duration::from_millis(100),
            max_backoff: Duration::from_secs(10),
            backoff_factor: 2.0,
        }
    }
}

/// Create a retry middleware with the given configuration.
///
/// The handler will be retried up to `config.max_attempts` times on
/// failure, with exponential backoff between attempts. The message is
/// cloned for each retry so `T` must implement [`Clone`].
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
        let mut last_err = None;
        for attempt in 0..self.config.max_attempts {
            let clone = msg.clone();
            match self.next.handle(clone).await {
                Ok(()) => return Ok(()),
                Err(e) => {
                    ::tracing::warn!(
                        attempt = attempt + 1,
                        max = self.config.max_attempts,
                        error = %e,
                        "handler attempt failed, will retry"
                    );
                    last_err = Some(e);
                    if attempt + 1 < self.config.max_attempts {
                        let delay = self.backoff(attempt);
                        tokio::time::sleep(delay).await;
                    }
                }
            }
        }
        Err(last_err.expect("at least one attempt must have been made"))
    }
}

impl<T: Send + Sync + 'static> RetryHandler<T> {
    fn backoff(&self, attempt: u32) -> Duration {
        let exp = self.config.backoff_factor.powi(attempt as i32);
        let base_ms = (self.config.initial_backoff.as_millis() as f64 * exp) as u64;
        let capped_ms = base_ms.min(self.config.max_backoff.as_millis() as u64);
        Duration::from_millis(capped_ms)
    }
}

#[cfg(test)]
mod tests {
    use std::sync::atomic::{AtomicU32, Ordering};

    use super::*;
    use crate::handler::{FnHandler, chain_handlers};
    use rskit_errors::{AppError, ErrorCode};

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

        let mw: Arc<dyn HandlerMiddleware<String>> = Arc::new(RetryMiddleware {
            config: RetryConfig {
                max_attempts: 3,
                initial_backoff: Duration::from_millis(1),
                ..Default::default()
            },
        });
        let handler = chain_handlers(base, &[mw]);

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

        let mw: Arc<dyn HandlerMiddleware<String>> = Arc::new(RetryMiddleware {
            config: RetryConfig {
                max_attempts: 3,
                initial_backoff: Duration::from_millis(1),
                ..Default::default()
            },
        });
        let handler = chain_handlers(base, &[mw]);

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

        let mw: Arc<dyn HandlerMiddleware<String>> = Arc::new(RetryMiddleware {
            config: RetryConfig {
                max_attempts: 3,
                initial_backoff: Duration::from_millis(1),
                ..Default::default()
            },
        });
        let handler = chain_handlers(base, &[mw]);

        let result = handler.handle(Message::new("t", "data".to_string())).await;
        assert!(result.is_err());
        assert_eq!(counter.load(Ordering::SeqCst), 3);
    }
}
