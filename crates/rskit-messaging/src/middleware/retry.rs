//! Retry middleware backed by [`rskit_resilience::RetryPolicy`].

use std::sync::Arc;

use async_trait::async_trait;
use rskit_errors::AppResult;
use rskit_resilience::RetryPolicy;

use crate::handler::{HandlerMiddleware, MessageHandler};
use crate::message::Message;

/// Canonical retry configuration for messaging middleware.
pub type RetryConfig = RetryPolicy;

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
        RetryPolicy::new()
            .with_max_attempts(3)
            .with_initial_backoff(Duration::from_millis(1))
            .with_jitter(false)
            .with_retry_if(|_| true)
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
}
