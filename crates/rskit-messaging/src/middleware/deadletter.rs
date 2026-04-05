//! Dead-letter queue middleware for failed messages.

use std::sync::Arc;

use async_trait::async_trait;
use rskit_errors::AppResult;

use crate::handler::{HandlerMiddleware, MessageHandler};
use crate::message::Message;
use crate::traits::MessageProducer;

/// Configuration for the dead-letter middleware.
#[derive(Debug, Clone)]
pub struct DeadLetterConfig {
    /// Suffix appended to the original topic to form the DLQ topic name.
    pub suffix: String,
}

impl Default for DeadLetterConfig {
    fn default() -> Self {
        Self {
            suffix: ".dlq".to_string(),
        }
    }
}

/// Create a dead-letter middleware that routes failed messages to a DLQ.
///
/// When the inner handler returns an error the original message is
/// forwarded to `<original_topic><suffix>` via the supplied `producer`,
/// and the original error is returned.
pub fn dead_letter<T: Send + Sync + Clone + 'static>(
    producer: Arc<dyn MessageProducer<T>>,
    config: DeadLetterConfig,
) -> impl HandlerMiddleware<T> {
    DeadLetterMiddleware { producer, config }
}

struct DeadLetterMiddleware<T: Send + Sync + 'static> {
    producer: Arc<dyn MessageProducer<T>>,
    config: DeadLetterConfig,
}

impl<T: Send + Sync + Clone + 'static> HandlerMiddleware<T> for DeadLetterMiddleware<T> {
    fn wrap(&self, next: Arc<dyn MessageHandler<T>>) -> Arc<dyn MessageHandler<T>> {
        Arc::new(DeadLetterHandler {
            producer: self.producer.clone(),
            suffix: self.config.suffix.clone(),
            next,
        })
    }
}

struct DeadLetterHandler<T: Send + Sync + 'static> {
    producer: Arc<dyn MessageProducer<T>>,
    suffix: String,
    next: Arc<dyn MessageHandler<T>>,
}

#[async_trait]
impl<T: Send + Sync + Clone + 'static> MessageHandler<T> for DeadLetterHandler<T> {
    async fn handle(&self, msg: Message<T>) -> AppResult<()> {
        let backup = msg.clone();
        match self.next.handle(msg).await {
            Ok(()) => Ok(()),
            Err(e) => {
                let dlq_topic = format!("{}{}", backup.topic, self.suffix);
                let dlq_msg = Message::new(dlq_topic, backup.payload);
                if let Err(send_err) = self.producer.send(dlq_msg).await {
                    ::tracing::error!(error = %send_err, "failed to send message to DLQ");
                }
                Err(e)
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use super::*;
    use crate::handler::{FnHandler, chain_handlers};
    use crate::memory::InMemoryBroker;
    use crate::traits::MessageConsumer;
    use rskit_errors::{AppError, ErrorCode};

    #[tokio::test]
    async fn success_does_not_produce_dlq() {
        let broker = InMemoryBroker::<String>::new(16);
        let dlq_producer = broker.producer();
        let dlq_consumer = broker.consumer();
        dlq_consumer.subscribe(&["topic.dlq"]).await.unwrap();

        let base: Arc<dyn MessageHandler<String>> =
            Arc::new(FnHandler::new(|_msg: Message<String>| async { Ok(()) }));

        let mw: Arc<dyn HandlerMiddleware<String>> = Arc::new(DeadLetterMiddleware {
            producer: Arc::new(dlq_producer),
            config: DeadLetterConfig::default(),
        });
        let handler = chain_handlers(base, &[mw]);

        handler
            .handle(Message::new("topic", "ok".to_string()))
            .await
            .unwrap();

        // Give a moment, then verify no DLQ message.
        let result = tokio::time::timeout(Duration::from_millis(50), dlq_consumer.recv()).await;
        assert!(result.is_err(), "should not have received a DLQ message");
    }

    #[tokio::test]
    async fn failure_routes_to_dlq() {
        let broker = InMemoryBroker::<String>::new(16);
        let dlq_producer = broker.producer();
        let dlq_consumer = broker.consumer();
        dlq_consumer.subscribe(&["topic.dlq"]).await.unwrap();

        let base: Arc<dyn MessageHandler<String>> =
            Arc::new(FnHandler::new(|_msg: Message<String>| async {
                Err(AppError::new(ErrorCode::Internal, "boom"))
            }));

        let mw: Arc<dyn HandlerMiddleware<String>> = Arc::new(DeadLetterMiddleware {
            producer: Arc::new(dlq_producer),
            config: DeadLetterConfig::default(),
        });
        let handler = chain_handlers(base, &[mw]);

        let result = handler
            .handle(Message::new("topic", "fail".to_string()))
            .await;
        assert!(result.is_err());

        let dlq_msg = tokio::time::timeout(Duration::from_millis(200), dlq_consumer.recv())
            .await
            .expect("should receive DLQ message");
        let dlq_msg = dlq_msg.unwrap();
        assert_eq!(dlq_msg.topic, "topic.dlq");
        assert_eq!(dlq_msg.payload, "fail");
    }
}
