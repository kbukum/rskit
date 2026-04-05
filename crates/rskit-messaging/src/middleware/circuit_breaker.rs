//! Circuit breaker middleware backed by [`rskit_resilience::CircuitBreaker`].

use std::sync::Arc;
use std::time::Duration;

use async_trait::async_trait;
use rskit_errors::AppResult;
use rskit_resilience::{CbConfig, CircuitBreaker};

use crate::handler::{HandlerMiddleware, MessageHandler};
use crate::message::Message;

/// Configuration for the circuit breaker middleware.
#[derive(Debug, Clone)]
pub struct CircuitBreakerConfig {
    /// Number of consecutive failures that trip the breaker.
    pub threshold: u32,
    /// How long the breaker stays open before entering half-open.
    pub timeout: Duration,
    /// Maximum probe calls allowed in half-open state.
    pub half_open_max: u32,
}

impl Default for CircuitBreakerConfig {
    fn default() -> Self {
        Self {
            threshold: 5,
            timeout: Duration::from_secs(30),
            half_open_max: 2,
        }
    }
}

/// Create a circuit breaker middleware.
///
/// Wraps the inner handler with a [`CircuitBreaker`] from
/// `rskit-resilience`. When consecutive failures exceed the configured
/// threshold the breaker opens and subsequent messages are rejected
/// immediately until the timeout elapses.
pub fn circuit_breaker<T: Send + Sync + 'static>(
    config: CircuitBreakerConfig,
) -> impl HandlerMiddleware<T> {
    let cb_config = CbConfig::new("messaging-cb")
        .with_max_failures(config.threshold as usize)
        .with_timeout(config.timeout)
        .with_half_open_max_calls(config.half_open_max as usize);
    CircuitBreakerMiddleware {
        cb: CircuitBreaker::new(cb_config),
        _marker: std::marker::PhantomData,
    }
}

struct CircuitBreakerMiddleware<T> {
    cb: CircuitBreaker,
    _marker: std::marker::PhantomData<T>,
}

// Safety: PhantomData is Send + Sync when T is, and CircuitBreaker is Send + Sync.
unsafe impl<T: Send> Send for CircuitBreakerMiddleware<T> {}
unsafe impl<T: Sync> Sync for CircuitBreakerMiddleware<T> {}

impl<T: Send + Sync + 'static> HandlerMiddleware<T> for CircuitBreakerMiddleware<T> {
    fn wrap(&self, next: Arc<dyn MessageHandler<T>>) -> Arc<dyn MessageHandler<T>> {
        Arc::new(CircuitBreakerHandler {
            cb: self.cb.clone(),
            next,
        })
    }
}

struct CircuitBreakerHandler<T: Send + Sync + 'static> {
    cb: CircuitBreaker,
    next: Arc<dyn MessageHandler<T>>,
}

#[async_trait]
impl<T: Send + Sync + 'static> MessageHandler<T> for CircuitBreakerHandler<T> {
    async fn handle(&self, msg: Message<T>) -> AppResult<()> {
        let next = self.next.clone();
        self.cb
            .execute(move || async move { next.handle(msg).await })
            .await
    }
}

#[cfg(test)]
mod tests {
    use std::sync::atomic::{AtomicU32, Ordering};

    use super::*;
    use crate::handler::{FnHandler, chain_handlers};
    use rskit_errors::{AppError, ErrorCode};
    use rskit_resilience::CbState;

    #[tokio::test]
    async fn passes_through_on_success() {
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

        let cb_config = CircuitBreakerConfig {
            threshold: 3,
            timeout: Duration::from_secs(30),
            half_open_max: 2,
        };
        let cb_mw = CircuitBreakerMiddleware {
            cb: CircuitBreaker::new(
                CbConfig::new("test")
                    .with_max_failures(cb_config.threshold as usize)
                    .with_timeout(cb_config.timeout)
                    .with_half_open_max_calls(cb_config.half_open_max as usize),
            ),
            _marker: std::marker::PhantomData::<String>,
        };
        let handler = chain_handlers(
            base,
            &[Arc::new(cb_mw) as Arc<dyn HandlerMiddleware<String>>],
        );

        handler
            .handle(Message::new("t", "ok".to_string()))
            .await
            .unwrap();
        assert_eq!(counter.load(Ordering::SeqCst), 1);
    }

    #[tokio::test]
    async fn opens_after_threshold_failures() {
        let base: Arc<dyn MessageHandler<String>> =
            Arc::new(FnHandler::new(|_msg: Message<String>| async {
                Err(AppError::new(ErrorCode::Internal, "fail"))
            }));

        let cb = CircuitBreaker::new(
            CbConfig::new("test")
                .with_max_failures(3)
                .with_timeout(Duration::from_secs(60)),
        );
        let cb_mw = CircuitBreakerMiddleware {
            cb: cb.clone(),
            _marker: std::marker::PhantomData::<String>,
        };
        let handler = chain_handlers(
            base,
            &[Arc::new(cb_mw) as Arc<dyn HandlerMiddleware<String>>],
        );

        for _ in 0..3 {
            let _ = handler.handle(Message::new("t", "data".to_string())).await;
        }
        assert_eq!(cb.state(), CbState::Open);

        // Next call should be rejected without invoking the handler.
        let result = handler.handle(Message::new("t", "data".to_string())).await;
        assert!(result.is_err());
    }
}
