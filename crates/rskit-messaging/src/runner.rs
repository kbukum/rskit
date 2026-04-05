//! Consumer runner that manages a consumption loop as a tokio task.

use std::sync::Arc;
use std::time::{Duration, Instant};

use rskit_errors::{AppError, AppResult, ErrorCode};
use tokio::task::JoinHandle;
use tokio_util::sync::CancellationToken;
use tracing;

use crate::handler::MessageHandler;
use crate::metrics::{MetricsCollector, NoopMetrics};
use crate::traits::MessageConsumer;

/// Manages the consumption loop as a tokio task.
///
/// Unlike [`ManagedConsumer`](crate::managed_consumer::ManagedConsumer),
/// `ConsumerRunner` owns the task handle directly and provides a simpler
/// run/stop interface without builder overhead.
pub struct ConsumerRunner<T: Send + Sync + Clone + 'static> {
    consumer: Arc<dyn MessageConsumer<T>>,
    handler: Arc<dyn MessageHandler<T>>,
    metrics: Arc<dyn MetricsCollector>,
    cancel: CancellationToken,
    handle: parking_lot::Mutex<Option<JoinHandle<()>>>,
    shutdown_timeout: Duration,
}

impl<T: Send + Sync + Clone + 'static> ConsumerRunner<T> {
    /// Create a new runner for the given consumer and handler.
    pub fn new(consumer: Arc<dyn MessageConsumer<T>>, handler: Arc<dyn MessageHandler<T>>) -> Self {
        Self {
            consumer,
            handler,
            metrics: Arc::new(NoopMetrics),
            cancel: CancellationToken::new(),
            handle: parking_lot::Mutex::new(None),
            shutdown_timeout: Duration::from_secs(10),
        }
    }

    /// Set the metrics collector.
    #[must_use]
    pub fn with_metrics(mut self, metrics: Arc<dyn MetricsCollector>) -> Self {
        self.metrics = metrics;
        self
    }

    /// Set the graceful shutdown timeout.
    #[must_use]
    pub fn with_shutdown_timeout(mut self, timeout: Duration) -> Self {
        self.shutdown_timeout = timeout;
        self
    }

    /// Returns `true` when the consumption loop is running.
    pub fn is_running(&self) -> bool {
        let guard = self.handle.lock();
        guard.as_ref().is_some_and(|h| !h.is_finished())
    }

    /// Start the consumption loop.
    pub fn run(&self) -> AppResult<()> {
        {
            let guard = self.handle.lock();
            if guard.as_ref().is_some_and(|h| !h.is_finished()) {
                return Err(AppError::new(
                    ErrorCode::InvalidInput,
                    "runner is already running",
                ));
            }
        }

        let consumer = self.consumer.clone();
        let handler = self.handler.clone();
        let metrics = self.metrics.clone();
        let cancel = self.cancel.clone();

        let task = tokio::spawn(async move {
            tracing::info!("consumer runner started");
            loop {
                tokio::select! {
                    _ = cancel.cancelled() => {
                        tracing::info!("consumer runner cancelled");
                        break;
                    }
                    result = consumer.recv() => {
                        match result {
                            Ok(msg) => {
                                let topic = msg.topic.clone();
                                let start = Instant::now();
                                let handle_result = handler.handle(msg).await;
                                metrics.record_consume(
                                    &topic,
                                    start.elapsed(),
                                    handle_result.is_ok(),
                                );
                                if let Err(e) = handle_result {
                                    tracing::warn!(error = %e, "handler error in runner");
                                }
                            }
                            Err(e) => {
                                if cancel.is_cancelled() {
                                    break;
                                }
                                tracing::error!(error = %e, "recv error in runner");
                                tokio::time::sleep(Duration::from_millis(100)).await;
                            }
                        }
                    }
                }
            }
            tracing::info!("consumer runner exited");
        });

        let mut guard = self.handle.lock();
        *guard = Some(task);

        Ok(())
    }

    /// Stop the consumption loop and wait for it to finish.
    pub async fn stop(&self) -> AppResult<()> {
        let handle = {
            let mut guard = self.handle.lock();
            guard.take()
        };

        let Some(h) = handle else {
            return Err(AppError::new(
                ErrorCode::InvalidInput,
                "runner is not running",
            ));
        };

        self.cancel.cancel();

        match tokio::time::timeout(self.shutdown_timeout, h).await {
            Ok(Ok(())) => {
                tracing::info!("consumer runner stopped");
            }
            Ok(Err(e)) => {
                tracing::warn!(error = %e, "runner task join error");
            }
            Err(_) => {
                tracing::warn!("runner shutdown timed out");
            }
        }

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use std::sync::atomic::{AtomicU32, Ordering};

    use super::*;
    use crate::handler::FnHandler;
    use crate::memory::InMemoryBroker;
    use crate::message::Message;
    use crate::traits::{self, MessageProducer};

    #[tokio::test]
    async fn runner_start_and_stop() {
        let broker = InMemoryBroker::<String>::new(16);
        let consumer = broker.consumer();

        let handler: Arc<dyn MessageHandler<String>> =
            Arc::new(FnHandler::new(|_msg: Message<String>| async { Ok(()) }));

        let runner = ConsumerRunner::new(Arc::new(consumer), handler);

        runner.run().unwrap();
        assert!(runner.is_running());

        runner.stop().await.unwrap();
        // After stop the task has finished
        assert!(!runner.is_running());
    }

    #[tokio::test]
    async fn runner_processes_messages() {
        let broker = InMemoryBroker::<String>::new(16);
        let producer = broker.producer();
        let consumer = broker.consumer();

        traits::MessageConsumer::subscribe(&consumer, &["topic"])
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

        let runner = ConsumerRunner::new(Arc::new(consumer), handler);
        runner.run().unwrap();

        producer
            .send(Message::new("topic", "msg1".to_string()))
            .await
            .unwrap();
        producer
            .send(Message::new("topic", "msg2".to_string()))
            .await
            .unwrap();

        tokio::time::sleep(Duration::from_millis(150)).await;
        assert_eq!(counter.load(Ordering::SeqCst), 2);

        runner.stop().await.unwrap();
    }

    #[tokio::test]
    async fn double_run_returns_error() {
        let broker = InMemoryBroker::<String>::new(16);
        let consumer = broker.consumer();

        let handler: Arc<dyn MessageHandler<String>> =
            Arc::new(FnHandler::new(|_msg: Message<String>| async { Ok(()) }));

        let runner = ConsumerRunner::new(Arc::new(consumer), handler);

        runner.run().unwrap();
        assert!(runner.run().is_err());

        runner.stop().await.unwrap();
    }

    #[tokio::test]
    async fn stop_when_not_running_returns_error() {
        let broker = InMemoryBroker::<String>::new(16);
        let consumer = broker.consumer();

        let handler: Arc<dyn MessageHandler<String>> =
            Arc::new(FnHandler::new(|_msg: Message<String>| async { Ok(()) }));

        let runner = ConsumerRunner::new(Arc::new(consumer), handler);
        assert!(runner.stop().await.is_err());
    }
}
