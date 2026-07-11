//! Handler-level metrics instrumentation middleware.

use std::sync::Arc;
use std::time::Instant;

use async_trait::async_trait;
use rskit_errors::AppResult;

use crate::handler::{HandlerMiddleware, MessageHandler};
use crate::message::Message;
use crate::metrics::MetricsCollector;

/// Create a middleware that records handler processing metrics.
///
/// Each handler invocation is timed and reported to the supplied
/// [`MetricsCollector`] via `record_consume`.
pub fn instrument<T: Send + Sync + 'static>(
    metrics: Arc<dyn MetricsCollector>,
    topic: String,
) -> impl HandlerMiddleware<T> {
    InstrumentMiddleware { metrics, topic }
}

struct InstrumentMiddleware {
    metrics: Arc<dyn MetricsCollector>,
    topic: String,
}

impl<T: Send + Sync + 'static> HandlerMiddleware<T> for InstrumentMiddleware {
    fn wrap(&self, next: Arc<dyn MessageHandler<T>>) -> Arc<dyn MessageHandler<T>> {
        Arc::new(InstrumentHandler {
            metrics: self.metrics.clone(),
            topic: self.topic.clone(),
            next,
        })
    }
}

struct InstrumentHandler<T: Send + Sync + 'static> {
    metrics: Arc<dyn MetricsCollector>,
    topic: String,
    next: Arc<dyn MessageHandler<T>>,
}

#[async_trait]
impl<T: Send + Sync + 'static> MessageHandler<T> for InstrumentHandler<T> {
    async fn handle(&self, msg: Message<T>) -> AppResult<()> {
        let start = Instant::now();
        let result = self.next.handle(msg).await;
        self.metrics
            .record_consume(&self.topic, start.elapsed(), result.is_ok());
        result
    }
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use super::*;
    use crate::handler::{FnHandler, chain_handlers};
    use rskit_errors::{AppError, ErrorCode};

    struct RecordingMetrics {
        calls: Arc<parking_lot::Mutex<Vec<(String, Duration, bool)>>>,
    }

    impl MetricsCollector for RecordingMetrics {
        fn record_publish(&self, _topic: &str, _duration: Duration, _success: bool) {}

        fn record_consume(&self, topic: &str, duration: Duration, success: bool) {
            self.calls
                .lock()
                .push((topic.to_string(), duration, success));
        }
    }

    #[tokio::test]
    async fn records_success_metric() {
        let calls = Arc::new(parking_lot::Mutex::new(Vec::new()));
        let metrics: Arc<dyn MetricsCollector> = Arc::new(RecordingMetrics {
            calls: calls.clone(),
        });

        let base: Arc<dyn MessageHandler<String>> =
            Arc::new(FnHandler::new(|_msg: Message<String>| async { Ok(()) }));

        let mw: Arc<dyn HandlerMiddleware<String>> = Arc::new(instrument(metrics, "test".into()));
        let handler = chain_handlers(base, &[mw]);

        handler
            .handle(Message::new("test", "ok".to_string()))
            .await
            .unwrap();

        let recorded = calls.lock().clone();
        assert_eq!(recorded.len(), 1);
        assert_eq!(recorded[0].0, "test");
        assert!(recorded[0].2); // success = true
    }

    #[tokio::test]
    async fn records_failure_metric() {
        let calls = Arc::new(parking_lot::Mutex::new(Vec::new()));
        let metrics: Arc<dyn MetricsCollector> = Arc::new(RecordingMetrics {
            calls: calls.clone(),
        });

        let base: Arc<dyn MessageHandler<String>> =
            Arc::new(FnHandler::new(|_msg: Message<String>| async {
                Err(AppError::new(ErrorCode::Internal, "fail"))
            }));

        let mw: Arc<dyn HandlerMiddleware<String>> = Arc::new(InstrumentMiddleware {
            metrics,
            topic: "test".to_string(),
        });
        let handler = chain_handlers(base, &[mw]);

        let _ = handler
            .handle(Message::new("test", "fail".to_string()))
            .await;

        let recorded = calls.lock().clone();
        assert_eq!(recorded.len(), 1);
        assert!(!recorded[0].2); // success = false
    }
}
