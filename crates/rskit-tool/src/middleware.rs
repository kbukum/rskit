//! Middleware — composable wrappers for callable tools.

use async_trait::async_trait;
use rskit_errors::AppResult;
use rskit_schema::ValidationResult;
use std::sync::Arc;
use std::time::Duration;

use crate::callable::Callable;
use crate::context::Context;
use crate::definition::Definition;
use crate::result::ToolResult;

/// A function that wraps a Callable, returning a new Callable.
pub type Middleware = Arc<dyn Fn(Box<dyn Callable>) -> Box<dyn Callable> + Send + Sync>;

/// Compose multiple middlewares into a single middleware.
pub fn chain(middlewares: Vec<Middleware>) -> Middleware {
    Arc::new(move |tool| {
        let mut result = tool;
        for mw in middlewares.iter().rev() {
            result = mw(result);
        }
        result
    })
}

/// Middleware that logs tool calls and their duration.
pub fn with_logging() -> Middleware {
    Arc::new(|tool| Box::new(LoggingWrapper { inner: tool.into() }))
}

/// Middleware that enforces a timeout on tool execution.
pub fn with_timeout(duration: Duration) -> Middleware {
    Arc::new(move |tool| {
        Box::new(TimeoutWrapper {
            inner: tool.into(),
            duration,
        })
    })
}

/// Middleware that validates input against the tool's schema before execution.
pub fn with_validation() -> Middleware {
    Arc::new(|tool| Box::new(ValidationWrapper { inner: tool.into() }))
}

/// Middleware that truncates results exceeding the size limit.
pub fn with_result_limit(max_bytes: usize) -> Middleware {
    Arc::new(move |tool| {
        Box::new(ResultLimitWrapper {
            inner: tool.into(),
            max_bytes,
        })
    })
}

// ── LoggingWrapper ──────────────────────────────────────────────────────────

struct LoggingWrapper {
    inner: Arc<dyn Callable>,
}

#[async_trait]
impl Callable for LoggingWrapper {
    fn definition(&self) -> &Definition {
        self.inner.definition()
    }

    fn validate(&self, input: &serde_json::Value) -> ValidationResult {
        self.inner.validate(input)
    }

    async fn call(&self, ctx: &Context, input: serde_json::Value) -> AppResult<ToolResult> {
        let name = self.definition().name.clone();
        tracing::info!(tool = %name, "tool.call.start");
        let start = std::time::Instant::now();

        match self.inner.call(ctx, input).await {
            Ok(result) => {
                let elapsed = start.elapsed();
                tracing::info!(tool = %name, elapsed_ms = %elapsed.as_millis(), "tool.call.done");
                Ok(result)
            }
            Err(e) => {
                let elapsed = start.elapsed();
                tracing::error!(tool = %name, elapsed_ms = %elapsed.as_millis(), error = %e, "tool.call.error");
                Err(e)
            }
        }
    }
}

// ── TimeoutWrapper ──────────────────────────────────────────────────────────

struct TimeoutWrapper {
    inner: Arc<dyn Callable>,
    duration: Duration,
}

#[async_trait]
impl Callable for TimeoutWrapper {
    fn definition(&self) -> &Definition {
        self.inner.definition()
    }

    fn validate(&self, input: &serde_json::Value) -> ValidationResult {
        self.inner.validate(input)
    }

    async fn call(&self, ctx: &Context, input: serde_json::Value) -> AppResult<ToolResult> {
        tokio::time::timeout(self.duration, self.inner.call(ctx, input))
            .await
            .map_err(|_| {
                rskit_errors::AppError::new(
                    rskit_errors::ErrorCode::Timeout,
                    format!(
                        "tool {:?} timed out after {:?}",
                        self.definition().name,
                        self.duration
                    ),
                )
            })?
    }
}

// ── ValidationWrapper ───────────────────────────────────────────────────────

struct ValidationWrapper {
    inner: Arc<dyn Callable>,
}

#[async_trait]
impl Callable for ValidationWrapper {
    fn definition(&self) -> &Definition {
        self.inner.definition()
    }

    fn validate(&self, input: &serde_json::Value) -> ValidationResult {
        self.inner.validate(input)
    }

    async fn call(&self, ctx: &Context, input: serde_json::Value) -> AppResult<ToolResult> {
        let vr = self.inner.validate(&input);
        if !vr.valid {
            let msgs: Vec<String> = vr.errors.iter().map(|e| e.to_string()).collect();
            return Err(rskit_errors::AppError::new(
                rskit_errors::ErrorCode::InvalidInput,
                format!("input validation failed: {}", msgs.join("; ")),
            ));
        }
        self.inner.call(ctx, input).await
    }
}

// ── ResultLimitWrapper ──────────────────────────────────────────────────────

struct ResultLimitWrapper {
    inner: Arc<dyn Callable>,
    max_bytes: usize,
}

#[async_trait]
impl Callable for ResultLimitWrapper {
    fn definition(&self) -> &Definition {
        self.inner.definition()
    }

    fn validate(&self, input: &serde_json::Value) -> ValidationResult {
        self.inner.validate(input)
    }

    async fn call(&self, ctx: &Context, input: serde_json::Value) -> AppResult<ToolResult> {
        let mut result = self.inner.call(ctx, input).await?;
        if self.max_bytes > 0 && result.content.len() > self.max_bytes {
            result.content.truncate(self.max_bytes);
            result.content.push_str("\n... [truncated to size limit]");
        }
        Ok(result)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::from_fn;
    use schemars::JsonSchema;
    use serde::{Deserialize, Serialize};

    #[derive(Deserialize, JsonSchema)]
    struct EchoInput {
        message: String,
    }

    #[derive(Serialize)]
    struct EchoOutput {
        echo: String,
    }

    fn make_echo_tool() -> Box<dyn Callable> {
        from_fn(
            "echo",
            "Echo tool",
            |_ctx: Context, input: EchoInput| async move {
                Ok(crate::result::text_result(&input.message))
            },
        )
    }

    #[tokio::test]
    async fn test_logging_middleware() {
        let tool = make_echo_tool();
        let logged = with_logging()(tool);
        let ctx = Context::new();
        let result = logged
            .call(&ctx, serde_json::json!({"message": "hello"}))
            .await
            .unwrap();
        assert_eq!(result.text(), "hello");
    }

    #[tokio::test]
    async fn test_timeout_success() {
        let tool = make_echo_tool();
        let timed = with_timeout(Duration::from_secs(5))(tool);
        let ctx = Context::new();
        let result = timed
            .call(&ctx, serde_json::json!({"message": "hi"}))
            .await
            .unwrap();
        assert_eq!(result.text(), "hi");
    }

    #[tokio::test]
    async fn test_timeout_exceeded() {
        let tool = from_fn(
            "slow",
            "Slow tool",
            |_ctx: Context, _input: EchoInput| async move {
                tokio::time::sleep(Duration::from_secs(10)).await;
                Ok(crate::result::text_result("done"))
            },
        );

        let timed = with_timeout(Duration::from_millis(10))(tool);
        let ctx = Context::new();
        let result = timed.call(&ctx, serde_json::json!({"message": "hi"})).await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_chain_middlewares() {
        let tool = make_echo_tool();
        let combined = chain(vec![with_logging(), with_timeout(Duration::from_secs(5))]);
        let wrapped = combined(tool);
        assert_eq!(wrapped.definition().name, "echo");

        let ctx = Context::new();
        let result = wrapped
            .call(&ctx, serde_json::json!({"message": "chained"}))
            .await
            .unwrap();
        assert_eq!(result.text(), "chained");
    }

    #[tokio::test]
    async fn test_validation_middleware_valid() {
        let tool = make_echo_tool();
        let validated = with_validation()(tool);
        let ctx = Context::new();
        let result = validated
            .call(&ctx, serde_json::json!({"message": "hello"}))
            .await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_validation_middleware_invalid() {
        let tool = make_echo_tool();
        let validated = with_validation()(tool);
        let ctx = Context::new();
        let result = validated.call(&ctx, serde_json::json!(42)).await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_result_limit() {
        let tool = from_fn(
            "big",
            "Big result",
            |_ctx: Context, _input: EchoInput| async move {
                Ok(crate::result::text_result(&"x".repeat(1000)))
            },
        );

        let limited = with_result_limit(100)(tool);
        let ctx = Context::new();
        let result = limited
            .call(&ctx, serde_json::json!({"message": "go"}))
            .await
            .unwrap();
        assert!(result.content.len() < 200);
        assert!(result.content.contains("[truncated"));
    }
}
