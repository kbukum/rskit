//! Managed consumer that wraps any [`MessageConsumer`] with lifecycle
//! management, handler dispatch, and graceful shutdown.

use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Instant;

use rskit_errors::{AppError, AppResult, ErrorCode};
use tokio_util::sync::CancellationToken;
use tracing;

use crate::handler::MessageHandler;
use crate::metrics::{MetricsCollector, NoopMetrics};
use crate::traits::MessageConsumer;

/// Wraps a [`MessageConsumer`] with lifecycle, handler dispatch, and
/// graceful shutdown.
///
/// Use [`ManagedConsumerBuilder`] to construct an instance.
pub struct ManagedConsumer<T: Send + Sync + Clone + 'static> {
    inner: Arc<dyn MessageConsumer<T>>,
    handler: Arc<dyn MessageHandler<T>>,
    metrics: Arc<dyn MetricsCollector>,
    name: String,
    cancel: CancellationToken,
    running: Arc<AtomicBool>,
    handle: parking_lot::Mutex<Option<tokio::task::JoinHandle<()>>>,
}

impl<T: Send + Sync + Clone + 'static> ManagedConsumer<T> {
    /// Returns the name of this managed consumer.
    pub fn name(&self) -> &str {
        &self.name
    }

    /// Returns `true` when the consumer loop is running.
    pub fn is_running(&self) -> bool {
        self.running.load(Ordering::SeqCst)
    }

    /// Start the consumption loop in a background tokio task.
    pub fn start(&self) -> AppResult<()> {
        if self.running.load(Ordering::SeqCst) {
            return Err(AppError::new(
                ErrorCode::InvalidInput,
                format!("consumer '{}' is already running", self.name),
            ));
        }

        let consumer = self.inner.clone();
        let handler = self.handler.clone();
        let metrics = self.metrics.clone();
        let cancel = self.cancel.clone();
        let running = self.running.clone();
        let name = self.name.clone();

        running.store(true, Ordering::SeqCst);

        let task = tokio::spawn(async move {
            tracing::info!(consumer = %name, "managed consumer loop started");
            let mut consecutive_errors: u32 = 0;
            loop {
                tokio::select! {
                    _ = cancel.cancelled() => {
                        tracing::info!(consumer = %name, "managed consumer cancelled");
                        break;
                    }
                    result = consumer.recv() => {
                        match result {
                            Ok(msg) => {
                                consecutive_errors = 0;
                                let topic = msg.topic.clone();
                                let start = Instant::now();
                                let handle_result = handler.handle(msg).await;
                                metrics.record_consume(
                                    &topic,
                                    start.elapsed(),
                                    handle_result.is_ok(),
                                );
                                if let Err(e) = handle_result {
                                    tracing::warn!(
                                        consumer = %name,
                                        error = %e,
                                        "handler error"
                                    );
                                }
                            }
                            Err(e) => {
                                if cancel.is_cancelled() {
                                    break;
                                }
                                consecutive_errors += 1;
                                // Log first error and then only every 10th to avoid spam
                                if consecutive_errors == 1 || consecutive_errors % 10 == 0 {
                                    tracing::warn!(
                                        consumer = %name,
                                        error = %e,
                                        consecutive = consecutive_errors,
                                        "recv error (retrying)"
                                    );
                                }
                                // Exponential backoff: 500ms, 1s, 2s, 4s, capped at 5s
                                let backoff_ms = (500u64 * (1u64 << consecutive_errors.min(3))).min(5000);
                                tokio::time::sleep(std::time::Duration::from_millis(backoff_ms)).await;
                            }
                        }
                    }
                }
            }
            running.store(false, Ordering::SeqCst);
            tracing::info!(consumer = %name, "managed consumer loop exited");
        });

        let mut guard = self.handle.lock();
        *guard = Some(task);

        Ok(())
    }

    /// Stop the consumption loop and wait for it to drain.
    pub async fn stop(&self) -> AppResult<()> {
        if !self.running.load(Ordering::SeqCst) {
            return Err(AppError::new(
                ErrorCode::InvalidInput,
                format!("consumer '{}' is not running", self.name),
            ));
        }

        self.cancel.cancel();

        let handle = {
            let mut guard = self.handle.lock();
            guard.take()
        };

        if let Some(h) = handle {
            let timeout = tokio::time::timeout(std::time::Duration::from_secs(10), h);
            match timeout.await {
                Ok(Ok(())) => {}
                Ok(Err(e)) => {
                    tracing::warn!(consumer = %self.name, error = %e, "task join error");
                }
                Err(_) => {
                    tracing::warn!(consumer = %self.name, "shutdown timed out");
                }
            }
        }

        self.running.store(false, Ordering::SeqCst);
        tracing::info!(consumer = %self.name, "managed consumer stopped");
        Ok(())
    }
}

/// Builder for [`ManagedConsumer`].
pub struct ManagedConsumerBuilder<T: Send + Sync + Clone + 'static> {
    inner: Arc<dyn MessageConsumer<T>>,
    handler: Arc<dyn MessageHandler<T>>,
    metrics: Arc<dyn MetricsCollector>,
    name: String,
}

impl<T: Send + Sync + Clone + 'static> ManagedConsumerBuilder<T> {
    /// Create a new builder wrapping the given consumer and handler.
    pub fn new(
        name: impl Into<String>,
        inner: Arc<dyn MessageConsumer<T>>,
        handler: Arc<dyn MessageHandler<T>>,
    ) -> Self {
        Self {
            inner,
            handler,
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

    /// Build the managed consumer.
    pub fn build(self) -> ManagedConsumer<T> {
        ManagedConsumer {
            inner: self.inner,
            handler: self.handler,
            metrics: self.metrics,
            name: self.name,
            cancel: CancellationToken::new(),
            running: Arc::new(AtomicBool::new(false)),
            handle: parking_lot::Mutex::new(None),
        }
    }
}

#[cfg(test)]
mod tests {
    use std::sync::atomic::AtomicU32;

    use super::*;
    use crate::handler::FnHandler;
    use crate::memory::InMemoryBroker;
    use crate::message::Message;
    use crate::traits::MessageProducer;

    #[tokio::test]
    async fn consumer_receives_and_handles_messages() {
        let broker = InMemoryBroker::<String>::new(16);
        let producer = broker.producer();
        let consumer = broker.consumer();

        // Subscribe before building managed consumer
        crate::traits::MessageConsumer::subscribe(&consumer, &["test"])
            .await
            .unwrap();

        let counter = Arc::new(AtomicU32::new(0));
        let c = counter.clone();
        let handler: Arc<dyn MessageHandler<String>> =
            Arc::new(FnHandler::new(move |_msg: Message<String>| {
                let c = c.clone();
                async move {
                    c.fetch_add(1, Ordering::SeqCst);
                    Ok(())
                }
            }));

        let managed = ManagedConsumerBuilder::new("test", Arc::new(consumer), handler).build();

        managed.start().unwrap();

        // Send a message
        producer
            .send(Message::new("test", "hello".to_string()))
            .await
            .unwrap();

        // Give the consumer loop time to process
        tokio::time::sleep(std::time::Duration::from_millis(100)).await;

        assert_eq!(counter.load(Ordering::SeqCst), 1);
        assert!(managed.is_running());

        managed.stop().await.unwrap();
        assert!(!managed.is_running());
    }

    #[tokio::test]
    async fn double_start_returns_error() {
        let broker = InMemoryBroker::<String>::new(16);
        let consumer = broker.consumer();

        let handler: Arc<dyn MessageHandler<String>> =
            Arc::new(FnHandler::new(|_msg: Message<String>| async { Ok(()) }));

        let managed = ManagedConsumerBuilder::new("test", Arc::new(consumer), handler).build();

        managed.start().unwrap();
        assert!(managed.start().is_err());

        managed.stop().await.unwrap();
    }

    #[tokio::test]
    async fn stop_when_not_running_returns_error() {
        let broker = InMemoryBroker::<String>::new(16);
        let consumer = broker.consumer();

        let handler: Arc<dyn MessageHandler<String>> =
            Arc::new(FnHandler::new(|_msg: Message<String>| async { Ok(()) }));

        let managed = ManagedConsumerBuilder::new("test", Arc::new(consumer), handler).build();

        assert!(managed.stop().await.is_err());
    }
}
