//! Metrics middleware — records tool call durations and errors.

use std::collections::HashMap;
use std::sync::Arc;
use std::time::{Duration, Instant};

use async_trait::async_trait;
use parking_lot::RwLock;
use rskit_errors::{AppError, AppResult};
use rskit_schema::ValidationResult;

use crate::callable::Callable;
use crate::context::Context;
use crate::definition::Definition;
use crate::middleware::Middleware;
use crate::result::ToolResult;

/// Collector interface for recording tool call metrics.
pub trait MetricsCollector: Send + Sync {
    /// Called after every tool execution with the tool name, elapsed duration,
    /// and an optional error reference.
    fn record_call(&self, tool_name: &str, duration: Duration, error: Option<&AppError>);
}

/// A simple in-memory metrics collector for testing and development.
#[derive(Debug, Default)]
pub struct InMemoryMetrics {
    records: RwLock<Vec<MetricRecord>>,
}

/// A single recorded metric entry.
#[derive(Debug, Clone)]
pub struct MetricRecord {
    /// Name of the tool that was called.
    pub tool_name: String,
    /// How long the call took.
    pub duration: Duration,
    /// Whether the call resulted in an error.
    pub is_error: bool,
}

impl InMemoryMetrics {
    /// Create a new empty metrics collector.
    pub fn new() -> Self {
        Self::default()
    }

    /// Return a snapshot of all recorded metrics.
    pub fn records(&self) -> Vec<MetricRecord> {
        self.records.read().clone()
    }

    /// Total number of recorded calls.
    pub fn call_count(&self) -> usize {
        self.records.read().len()
    }

    /// Number of calls that ended in error.
    pub fn error_count(&self) -> usize {
        self.records.read().iter().filter(|r| r.is_error).count()
    }

    /// Average duration of all recorded calls.
    pub fn avg_duration(&self) -> Duration {
        let records = self.records.read();
        if records.is_empty() {
            return Duration::ZERO;
        }
        let total: Duration = records.iter().map(|r| r.duration).sum();
        total / records.len() as u32
    }

    /// Return metrics grouped by tool name: `(call_count, error_count)`.
    pub fn by_tool(&self) -> HashMap<String, (usize, usize)> {
        let records = self.records.read();
        let mut map: HashMap<String, (usize, usize)> = HashMap::new();
        for r in records.iter() {
            let entry = map.entry(r.tool_name.clone()).or_insert((0, 0));
            entry.0 += 1;
            if r.is_error {
                entry.1 += 1;
            }
        }
        map
    }
}

impl MetricsCollector for InMemoryMetrics {
    fn record_call(&self, tool_name: &str, duration: Duration, error: Option<&AppError>) {
        self.records.write().push(MetricRecord {
            tool_name: tool_name.to_string(),
            duration,
            is_error: error.is_some(),
        });
    }
}

/// Create a middleware that records metrics for every tool call.
pub fn with_metrics(collector: Arc<dyn MetricsCollector>) -> Middleware {
    Arc::new(move |tool| {
        Box::new(MetricsWrapper {
            inner: tool.into(),
            collector: collector.clone(),
        })
    })
}

struct MetricsWrapper {
    inner: Arc<dyn Callable>,
    collector: Arc<dyn MetricsCollector>,
}

#[async_trait]
impl Callable for MetricsWrapper {
    fn definition(&self) -> &Definition {
        self.inner.definition()
    }

    fn validate(&self, input: &serde_json::Value) -> ValidationResult {
        self.inner.validate(input)
    }

    async fn call(&self, ctx: &Context, input: serde_json::Value) -> AppResult<ToolResult> {
        let name = self.definition().name.clone();
        let start = Instant::now();

        match self.inner.call(ctx, input).await {
            Ok(result) => {
                self.collector.record_call(&name, start.elapsed(), None);
                Ok(result)
            }
            Err(err) => {
                self.collector
                    .record_call(&name, start.elapsed(), Some(&err));
                Err(err)
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{Context, from_fn, result::text_result};
    use schemars::JsonSchema;
    use serde::Deserialize;

    #[derive(Deserialize, JsonSchema)]
    struct EmptyInput {}

    #[tokio::test]
    async fn test_metrics_records_success() {
        let metrics = Arc::new(InMemoryMetrics::new());
        let tool = from_fn("add", "Add", |_ctx: Context, _: EmptyInput| async {
            Ok(text_result("ok"))
        });

        let wrapped = with_metrics(metrics.clone())(tool);
        let ctx = Context::new();
        wrapped.call(&ctx, serde_json::json!({})).await.unwrap();

        assert_eq!(metrics.call_count(), 1);
        assert_eq!(metrics.error_count(), 0);

        let records = metrics.records();
        assert_eq!(records[0].tool_name, "add");
        assert!(!records[0].is_error);
    }

    #[tokio::test]
    async fn test_metrics_records_error() {
        let metrics = Arc::new(InMemoryMetrics::new());
        let tool = from_fn("fail", "Fail", |_ctx: Context, _: EmptyInput| async {
            Err(AppError::new(rskit_errors::ErrorCode::Internal, "boom"))
        });

        let wrapped = with_metrics(metrics.clone())(tool);
        let ctx = Context::new();
        let _ = wrapped.call(&ctx, serde_json::json!({})).await;

        assert_eq!(metrics.call_count(), 1);
        assert_eq!(metrics.error_count(), 1);
    }

    #[tokio::test]
    async fn test_metrics_multiple_calls() {
        let metrics = Arc::new(InMemoryMetrics::new());
        let tool = from_fn("echo", "Echo", |_ctx: Context, _: EmptyInput| async {
            Ok(text_result("hi"))
        });

        let wrapped = with_metrics(metrics.clone())(tool);
        let ctx = Context::new();
        for _ in 0..5 {
            wrapped.call(&ctx, serde_json::json!({})).await.unwrap();
        }

        assert_eq!(metrics.call_count(), 5);
        assert_eq!(metrics.error_count(), 0);
        assert!(metrics.avg_duration() >= Duration::ZERO);
    }

    #[tokio::test]
    async fn test_metrics_by_tool() {
        let metrics = Arc::new(InMemoryMetrics::new());

        let tool_a = from_fn("alpha", "A", |_ctx: Context, _: EmptyInput| async {
            Ok(text_result("a"))
        });
        let tool_b = from_fn("beta", "B", |_ctx: Context, _: EmptyInput| async {
            Err(AppError::new(rskit_errors::ErrorCode::Internal, "b"))
        });

        let wrapped_a = with_metrics(metrics.clone())(tool_a);
        let wrapped_b = with_metrics(metrics.clone())(tool_b);
        let ctx = Context::new();

        wrapped_a.call(&ctx, serde_json::json!({})).await.unwrap();
        wrapped_a.call(&ctx, serde_json::json!({})).await.unwrap();
        let _ = wrapped_b.call(&ctx, serde_json::json!({})).await;

        let by_tool = metrics.by_tool();
        assert_eq!(by_tool["alpha"], (2, 0));
        assert_eq!(by_tool["beta"], (1, 1));
    }

    #[test]
    fn test_in_memory_metrics_default_empty() {
        let metrics = InMemoryMetrics::new();
        assert_eq!(metrics.call_count(), 0);
        assert_eq!(metrics.error_count(), 0);
        assert_eq!(metrics.avg_duration(), Duration::ZERO);
        assert!(metrics.records().is_empty());
        assert!(metrics.by_tool().is_empty());
    }
}
