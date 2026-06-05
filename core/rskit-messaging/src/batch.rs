//! Batch producer that buffers messages and flushes by size or interval.

use std::sync::Arc;
use std::time::Duration;

use rskit_errors::{AppError, AppResult, ErrorCode};
use tokio::task::JoinHandle;
use tokio_util::sync::CancellationToken;

use crate::message::Message;
use crate::traits::MessageProducer;

/// Configuration for [`BatchProducer`].
#[derive(Debug, Clone)]
pub struct BatchConfig {
    /// Maximum number of messages before a flush is triggered.
    pub max_size: usize,
    /// Maximum time between automatic flushes.
    pub max_wait: Duration,
    /// Maximum byte budget per batch (reserved for specialized
    /// implementations; not enforced in the generic `BatchProducer`).
    pub max_bytes: Option<u64>,
}

impl Default for BatchConfig {
    fn default() -> Self {
        Self {
            max_size: 100,
            max_wait: Duration::from_secs(5),
            max_bytes: None,
        }
    }
}

impl BatchConfig {
    /// Validate batch bounds before spawning background flush tasks.
    pub fn validate(&self) -> AppResult<()> {
        if self.max_size == 0 {
            return Err(AppError::new(
                ErrorCode::InvalidInput,
                "batch max_size must be greater than zero",
            ));
        }
        if self.max_wait.is_zero() {
            return Err(AppError::new(
                ErrorCode::InvalidInput,
                "batch max_wait must be greater than zero",
            ));
        }
        Ok(())
    }
}

/// A producer that buffers messages and flushes them in batches.
///
/// Flushing occurs when:
/// - The buffer reaches [`BatchConfig::max_size`] messages.
/// - The [`BatchConfig::max_wait`] interval elapses.
/// - [`BatchProducer::flush`] or [`BatchProducer::close`] is called.
///
/// A background tokio task handles periodic flushing.
pub struct BatchProducer<T: Send + Sync + Clone + 'static> {
    producer: Arc<dyn MessageProducer<T>>,
    config: BatchConfig,
    buffer: Arc<tokio::sync::Mutex<Vec<Message<T>>>>,
    cancel: CancellationToken,
    flush_handle: Arc<tokio::sync::Mutex<Option<JoinHandle<()>>>>,
}

impl<T: Send + Sync + Clone + 'static> BatchProducer<T> {
    /// Create a new batch producer.
    ///
    /// Spawns a background task that periodically flushes buffered
    /// messages according to `config.max_wait`.
    pub fn new(
        producer: Arc<dyn MessageProducer<T>>,
        _topic: String,
        config: BatchConfig,
    ) -> AppResult<Self> {
        config.validate()?;
        let buffer: Arc<tokio::sync::Mutex<Vec<Message<T>>>> =
            Arc::new(tokio::sync::Mutex::new(Vec::new()));
        let cancel = CancellationToken::new();

        let flush_buffer = buffer.clone();
        let flush_producer = producer.clone();
        let flush_cancel = cancel.clone();
        let max_wait = config.max_wait;

        let handle = tokio::spawn(async move {
            let mut interval = tokio::time::interval(max_wait);
            // Consume the immediate first tick.
            interval.tick().await;
            loop {
                tokio::select! {
                    _ = flush_cancel.cancelled() => break,
                    _ = interval.tick() => {
                        let msgs = {
                            let mut buf = flush_buffer.lock().await;
                            std::mem::take(&mut *buf)
                        };
                        if !msgs.is_empty()
                            && let Err(e) = flush_producer.send_batch(msgs).await {
                                ::tracing::error!(error = %e, "batch periodic flush failed");
                            }
                    }
                }
            }
        });

        Ok(Self {
            producer,
            config,
            buffer,
            cancel,
            flush_handle: Arc::new(tokio::sync::Mutex::new(Some(handle))),
        })
    }

    /// Buffer a message, flushing immediately if `max_size` is reached.
    pub async fn send(&self, msg: Message<T>) -> AppResult<()> {
        let should_flush = {
            let mut buf = self.buffer.lock().await;
            buf.push(msg);
            buf.len() >= self.config.max_size
        };
        if should_flush {
            self.flush().await?;
        }
        Ok(())
    }

    /// Flush all buffered messages to the underlying producer.
    pub async fn flush(&self) -> AppResult<()> {
        let msgs = {
            let mut buf = self.buffer.lock().await;
            std::mem::take(&mut *buf)
        };
        if !msgs.is_empty() {
            self.producer.send_batch(msgs).await?;
        }
        Ok(())
    }

    /// Flush remaining messages and shut down the background flush task.
    pub async fn close(&self) -> AppResult<()> {
        self.cancel.cancel();
        self.flush().await?;
        let handle = {
            let mut h = self.flush_handle.lock().await;
            h.take()
        };
        if let Some(h) = handle {
            let _ = h.await;
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use async_trait::async_trait;

    /// Producer that records every message sent through it.
    struct RecordingProducer<T: Clone + Send + Sync + 'static> {
        sent: Arc<tokio::sync::Mutex<Vec<Message<T>>>>,
    }

    impl<T: Clone + Send + Sync + 'static> RecordingProducer<T> {
        fn new() -> (Self, Arc<tokio::sync::Mutex<Vec<Message<T>>>>) {
            let sent = Arc::new(tokio::sync::Mutex::new(Vec::new()));
            (Self { sent: sent.clone() }, sent)
        }
    }

    #[async_trait]
    impl<T: Clone + Send + Sync + 'static> MessageProducer<T> for RecordingProducer<T> {
        async fn send(&self, msg: Message<T>) -> AppResult<()> {
            self.sent.lock().await.push(msg);
            Ok(())
        }

        async fn send_batch(&self, msgs: Vec<Message<T>>) -> AppResult<()> {
            self.sent.lock().await.extend(msgs);
            Ok(())
        }

        async fn flush(&self, _timeout: Duration) -> AppResult<()> {
            Ok(())
        }
    }

    #[tokio::test]
    async fn batch_flush_on_size() {
        let (producer, sent) = RecordingProducer::<String>::new();
        let batch = BatchProducer::new(
            Arc::new(producer),
            "topic".to_string(),
            BatchConfig {
                max_size: 3,
                max_wait: Duration::from_secs(60),
                max_bytes: None,
            },
        )
        .unwrap();

        batch
            .send(Message::new("topic", "a".to_string()))
            .await
            .unwrap();
        batch
            .send(Message::new("topic", "b".to_string()))
            .await
            .unwrap();
        assert_eq!(sent.lock().await.len(), 0);

        // Third message should trigger a size-based flush.
        batch
            .send(Message::new("topic", "c".to_string()))
            .await
            .unwrap();
        assert_eq!(sent.lock().await.len(), 3);

        batch.close().await.unwrap();
    }

    #[tokio::test(start_paused = true)]
    async fn batch_flush_on_time() {
        let (producer, sent) = RecordingProducer::<String>::new();
        let config = BatchConfig {
            max_size: 100,
            max_wait: Duration::from_millis(100),
            max_bytes: None,
        };
        let batch = BatchProducer::new(Arc::new(producer), "topic".to_string(), config).unwrap();
        tokio::task::yield_now().await;

        batch
            .send(Message::new("topic", "a".to_string()))
            .await
            .unwrap();
        batch
            .send(Message::new("topic", "b".to_string()))
            .await
            .unwrap();
        assert_eq!(sent.lock().await.len(), 0);

        tokio::time::advance(Duration::from_millis(101)).await;
        tokio::task::yield_now().await;
        tokio::task::yield_now().await;

        assert_eq!(sent.lock().await.len(), 2);
        batch.close().await.unwrap();
    }

    #[tokio::test]
    async fn batch_flush_on_close() {
        let (producer, sent) = RecordingProducer::<String>::new();
        let batch = BatchProducer::new(
            Arc::new(producer),
            "topic".to_string(),
            BatchConfig {
                max_size: 100,
                max_wait: Duration::from_secs(60),
                max_bytes: None,
            },
        )
        .unwrap();

        batch
            .send(Message::new("topic", "a".to_string()))
            .await
            .unwrap();
        batch
            .send(Message::new("topic", "b".to_string()))
            .await
            .unwrap();
        assert_eq!(sent.lock().await.len(), 0);

        batch.close().await.unwrap();
        assert_eq!(sent.lock().await.len(), 2);
    }

    #[test]
    fn batch_config_rejects_unbounded_zero_values() {
        assert!(
            BatchConfig {
                max_size: 0,
                ..BatchConfig::default()
            }
            .validate()
            .is_err()
        );
        assert!(
            BatchConfig {
                max_wait: Duration::ZERO,
                ..BatchConfig::default()
            }
            .validate()
            .is_err()
        );
    }
}
