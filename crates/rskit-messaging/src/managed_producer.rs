//! Managed producer that wraps any [`MessageProducer`] with lifecycle
//! management and metrics collection.

use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};

use async_trait::async_trait;
use rskit_errors::{AppError, AppResult, ErrorCode};

use crate::message::Message;
use crate::metrics::{MetricsCollector, NoopMetrics};
use crate::traits::MessageProducer;

/// Wraps a [`MessageProducer`] with lifecycle management and metrics.
///
/// Use [`ManagedProducerBuilder`] to construct an instance.
pub struct ManagedProducer<T: Send + Sync + 'static> {
    inner: Arc<dyn MessageProducer<T>>,
    metrics: Arc<dyn MetricsCollector>,
    name: String,
    running: AtomicBool,
}

impl<T: Send + Sync + 'static> ManagedProducer<T> {
    /// Returns the name of this managed producer.
    pub fn name(&self) -> &str {
        &self.name
    }

    /// Returns `true` when the producer is in the running state.
    pub fn is_running(&self) -> bool {
        self.running.load(Ordering::SeqCst)
    }

    /// Start the managed producer.
    pub fn start(&self) -> AppResult<()> {
        if self.running.swap(true, Ordering::SeqCst) {
            return Err(AppError::new(
                ErrorCode::InvalidInput,
                format!("producer '{}' is already running", self.name),
            ));
        }
        tracing::info!(producer = %self.name, "managed producer started");
        Ok(())
    }

    /// Stop the managed producer.
    pub fn stop(&self) -> AppResult<()> {
        if !self.running.swap(false, Ordering::SeqCst) {
            return Err(AppError::new(
                ErrorCode::InvalidInput,
                format!("producer '{}' is not running", self.name),
            ));
        }
        tracing::info!(producer = %self.name, "managed producer stopped");
        Ok(())
    }

    fn ensure_running(&self) -> AppResult<()> {
        if !self.is_running() {
            return Err(AppError::new(
                ErrorCode::InvalidInput,
                format!("producer '{}' is not running", self.name),
            ));
        }
        Ok(())
    }
}

#[async_trait]
impl<T: Send + Sync + 'static> MessageProducer<T> for ManagedProducer<T> {
    async fn send(&self, msg: Message<T>) -> AppResult<()> {
        self.ensure_running()?;
        let topic = msg.topic.clone();
        let start = Instant::now();
        let result = self.inner.send(msg).await;
        self.metrics
            .record_publish(&topic, start.elapsed(), result.is_ok());
        result
    }

    async fn send_batch(&self, msgs: Vec<Message<T>>) -> AppResult<()> {
        self.ensure_running()?;
        let topic = msgs
            .first()
            .map_or("unknown", |m| m.topic.as_str())
            .to_string();
        let start = Instant::now();
        let result = self.inner.send_batch(msgs).await;
        self.metrics
            .record_publish(&topic, start.elapsed(), result.is_ok());
        result
    }

    async fn flush(&self, timeout: Duration) -> AppResult<()> {
        self.ensure_running()?;
        self.inner.flush(timeout).await
    }
}

/// Builder for [`ManagedProducer`].
pub struct ManagedProducerBuilder<T: Send + Sync + 'static> {
    inner: Arc<dyn MessageProducer<T>>,
    metrics: Arc<dyn MetricsCollector>,
    name: String,
}

impl<T: Send + Sync + 'static> ManagedProducerBuilder<T> {
    /// Create a new builder wrapping the given producer.
    pub fn new(name: impl Into<String>, inner: Arc<dyn MessageProducer<T>>) -> Self {
        Self {
            inner,
            metrics: Arc::new(NoopMetrics),
            name: name.into(),
        }
    }

    /// Set the metrics collector.
    #[must_use]
    pub fn with_metrics(mut self, metrics: Arc<dyn MetricsCollector>) -> Self {
        self.metrics = metrics;
        self
    }

    /// Build the managed producer.
    pub fn build(self) -> ManagedProducer<T> {
        ManagedProducer {
            inner: self.inner,
            metrics: self.metrics,
            name: self.name,
            running: AtomicBool::new(false),
        }
    }
}

#[cfg(test)]
mod tests {
    use std::sync::atomic::AtomicU32;

    use super::*;
    use crate::memory::InMemoryBroker;

    /// Metrics collector that counts publish calls.
    struct CountingMetrics {
        publish_count: AtomicU32,
    }

    impl CountingMetrics {
        fn new() -> Self {
            Self {
                publish_count: AtomicU32::new(0),
            }
        }
    }

    impl MetricsCollector for CountingMetrics {
        fn record_publish(&self, _topic: &str, _duration: Duration, _success: bool) {
            self.publish_count.fetch_add(1, Ordering::SeqCst);
        }

        fn record_consume(&self, _topic: &str, _duration: Duration, _success: bool) {}
    }

    #[tokio::test]
    async fn lifecycle_start_stop() {
        let broker = InMemoryBroker::<String>::new(16);
        let inner = broker.producer();
        let producer = ManagedProducerBuilder::new("test", Arc::new(inner)).build();

        assert!(!producer.is_running());
        producer.start().unwrap();
        assert!(producer.is_running());
        producer.stop().unwrap();
        assert!(!producer.is_running());
    }

    #[tokio::test]
    async fn double_start_returns_error() {
        let broker = InMemoryBroker::<String>::new(16);
        let inner = broker.producer();
        let producer = ManagedProducerBuilder::new("test", Arc::new(inner)).build();

        producer.start().unwrap();
        assert!(producer.start().is_err());
    }

    #[tokio::test]
    async fn send_while_stopped_returns_error() {
        let broker = InMemoryBroker::<String>::new(16);
        let inner = broker.producer();
        let producer = ManagedProducerBuilder::new("test", Arc::new(inner)).build();

        let msg = Message::new("t", "hello".to_string());
        assert!(producer.send(msg).await.is_err());
    }

    #[tokio::test]
    async fn send_records_metrics() {
        let broker = InMemoryBroker::<String>::new(16);
        let _consumer = broker.consumer(); // keep a consumer so send doesn't fail
        let inner = broker.producer();
        let metrics = Arc::new(CountingMetrics::new());

        let producer = ManagedProducerBuilder::new("test", Arc::new(inner))
            .with_metrics(metrics.clone())
            .build();

        producer.start().unwrap();
        let msg = Message::new("t", "hello".to_string());
        producer.send(msg).await.unwrap();

        assert_eq!(metrics.publish_count.load(Ordering::SeqCst), 1);
    }
}
