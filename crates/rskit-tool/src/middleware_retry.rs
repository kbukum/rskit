//! Retry middleware — automatically retries failed tool calls with exponential backoff.

use std::sync::Arc;
use std::time::Duration;

use async_trait::async_trait;
use rskit_errors::{AppError, AppResult};
use rskit_schema::ValidationResult;

use crate::callable::Callable;
use crate::context::Context;
use crate::definition::Definition;
use crate::middleware::Middleware;
use crate::result::ToolResult;

/// Predicate that decides whether a given error should trigger a retry.
pub type RetryPredicate = Box<dyn Fn(&AppError) -> bool + Send + Sync>;

/// Configuration for the retry middleware.
pub struct RetryConfig {
    /// Maximum number of attempts (including the first call).
    pub max_attempts: u32,
    /// Initial delay between retries.
    pub base_delay: Duration,
    /// Upper bound on the delay (caps exponential growth).
    pub max_delay: Duration,
    /// Optional predicate to decide whether an error is retryable.
    /// When `None`, defaults to [`AppError::is_retryable`].
    pub should_retry: Option<RetryPredicate>,
}

impl Default for RetryConfig {
    fn default() -> Self {
        Self {
            max_attempts: 3,
            base_delay: Duration::from_millis(100),
            max_delay: Duration::from_secs(10),
            should_retry: None,
        }
    }
}

/// Create a middleware that retries failed tool calls with exponential backoff.
pub fn with_retry(config: RetryConfig) -> Middleware {
    let config = Arc::new(config);
    Arc::new(move |tool| {
        Box::new(RetryWrapper {
            inner: tool.into(),
            config: config.clone(),
        })
    })
}

struct RetryWrapper {
    inner: Arc<dyn Callable>,
    config: Arc<RetryConfig>,
}

#[async_trait]
impl Callable for RetryWrapper {
    fn definition(&self) -> &Definition {
        self.inner.definition()
    }

    fn validate(&self, input: &serde_json::Value) -> ValidationResult {
        self.inner.validate(input)
    }

    async fn call(&self, ctx: &Context, input: serde_json::Value) -> AppResult<ToolResult> {
        let mut last_err: Option<AppError> = None;

        for attempt in 0..self.config.max_attempts {
            match self.inner.call(ctx, input.clone()).await {
                Ok(result) => return Ok(result),
                Err(err) => {
                    let retryable = match &self.config.should_retry {
                        Some(predicate) => predicate(&err),
                        None => err.is_retryable(),
                    };

                    if !retryable || attempt + 1 >= self.config.max_attempts {
                        return Err(err);
                    }

                    let delay =
                        compute_delay(attempt, self.config.base_delay, self.config.max_delay);
                    tracing::warn!(
                        tool = %self.definition().name,
                        attempt = attempt + 1,
                        max_attempts = self.config.max_attempts,
                        delay_ms = %delay.as_millis(),
                        error = %err,
                        "tool.call.retrying"
                    );
                    tokio::time::sleep(delay).await;
                    last_err = Some(err);
                }
            }
        }

        Err(last_err.unwrap_or_else(|| {
            AppError::new(
                rskit_errors::ErrorCode::Internal,
                "retry loop exhausted without error",
            )
        }))
    }
}

/// Exponential backoff: base_delay * 2^attempt, capped at max_delay.
fn compute_delay(attempt: u32, base_delay: Duration, max_delay: Duration) -> Duration {
    let delay = base_delay.saturating_mul(2u32.saturating_pow(attempt));
    delay.min(max_delay)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{Context, from_fn, result::text_result};
    use schemars::JsonSchema;
    use serde::Deserialize;
    use std::sync::atomic::{AtomicU32, Ordering};

    #[derive(Deserialize, JsonSchema)]
    struct EmptyInput {}

    #[tokio::test]
    async fn test_retry_success_first_attempt() {
        let tool = from_fn(
            "ok",
            "Always succeeds",
            |_ctx: Context, _: EmptyInput| async { Ok(text_result("done")) },
        );

        let wrapped = with_retry(RetryConfig::default())(tool);
        let ctx = Context::new();
        let result = wrapped.call(&ctx, serde_json::json!({})).await.unwrap();
        assert_eq!(result.text(), "done");
    }

    #[tokio::test]
    async fn test_retry_succeeds_after_failures() {
        let counter = Arc::new(AtomicU32::new(0));
        let counter_clone = counter.clone();

        let tool = from_fn(
            "flaky",
            "Fails twice then succeeds",
            move |_ctx: Context, _: EmptyInput| {
                let c = counter_clone.clone();
                async move {
                    let n = c.fetch_add(1, Ordering::SeqCst);
                    if n < 2 {
                        Err(AppError::service_unavailable("test-service"))
                    } else {
                        Ok(text_result("recovered"))
                    }
                }
            },
        );

        let config = RetryConfig {
            max_attempts: 3,
            base_delay: Duration::from_millis(1),
            max_delay: Duration::from_millis(10),
            should_retry: None,
        };
        let wrapped = with_retry(config)(tool);
        let ctx = Context::new();
        let result = wrapped.call(&ctx, serde_json::json!({})).await.unwrap();
        assert_eq!(result.text(), "recovered");
        assert_eq!(counter.load(Ordering::SeqCst), 3);
    }

    #[tokio::test]
    async fn test_retry_exhausted() {
        let tool = from_fn(
            "fail",
            "Always fails",
            |_ctx: Context, _: EmptyInput| async { Err(AppError::service_unavailable("down")) },
        );

        let config = RetryConfig {
            max_attempts: 2,
            base_delay: Duration::from_millis(1),
            max_delay: Duration::from_millis(10),
            should_retry: None,
        };
        let wrapped = with_retry(config)(tool);
        let ctx = Context::new();
        let result = wrapped.call(&ctx, serde_json::json!({})).await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_retry_non_retryable_error() {
        let counter = Arc::new(AtomicU32::new(0));
        let counter_clone = counter.clone();

        let tool = from_fn(
            "notfound",
            "Returns not-found",
            move |_ctx: Context, _: EmptyInput| {
                let c = counter_clone.clone();
                async move {
                    c.fetch_add(1, Ordering::SeqCst);
                    Err(AppError::not_found("item", Some("123")))
                }
            },
        );

        let config = RetryConfig {
            max_attempts: 3,
            base_delay: Duration::from_millis(1),
            max_delay: Duration::from_millis(10),
            should_retry: None,
        };
        let wrapped = with_retry(config)(tool);
        let ctx = Context::new();
        let result = wrapped.call(&ctx, serde_json::json!({})).await;
        assert!(result.is_err());
        // Should not retry a non-retryable error
        assert_eq!(counter.load(Ordering::SeqCst), 1);
    }

    #[tokio::test]
    async fn test_retry_custom_predicate() {
        let counter = Arc::new(AtomicU32::new(0));
        let counter_clone = counter.clone();

        let tool = from_fn(
            "custom",
            "Custom retry",
            move |_ctx: Context, _: EmptyInput| {
                let c = counter_clone.clone();
                async move {
                    c.fetch_add(1, Ordering::SeqCst);
                    Err(AppError::not_found("item", Some("123")))
                }
            },
        );

        let config = RetryConfig {
            max_attempts: 3,
            base_delay: Duration::from_millis(1),
            max_delay: Duration::from_millis(10),
            should_retry: Some(Box::new(|_| true)), // always retry
        };
        let wrapped = with_retry(config)(tool);
        let ctx = Context::new();
        let result = wrapped.call(&ctx, serde_json::json!({})).await;
        assert!(result.is_err());
        assert_eq!(counter.load(Ordering::SeqCst), 3);
    }

    #[test]
    fn test_compute_delay() {
        let base = Duration::from_millis(100);
        let max = Duration::from_secs(5);

        assert_eq!(compute_delay(0, base, max), Duration::from_millis(100));
        assert_eq!(compute_delay(1, base, max), Duration::from_millis(200));
        assert_eq!(compute_delay(2, base, max), Duration::from_millis(400));
        assert_eq!(compute_delay(3, base, max), Duration::from_millis(800));

        // Caps at max_delay
        assert_eq!(compute_delay(20, base, max), Duration::from_secs(5));
    }

    #[test]
    fn test_default_config() {
        let config = RetryConfig::default();
        assert_eq!(config.max_attempts, 3);
        assert_eq!(config.base_delay, Duration::from_millis(100));
        assert_eq!(config.max_delay, Duration::from_secs(10));
        assert!(config.should_retry.is_none());
    }
}
