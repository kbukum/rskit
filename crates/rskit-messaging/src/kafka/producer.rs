use std::time::Duration;

use async_trait::async_trait;
use rdkafka::producer::{FutureProducer, FutureRecord, Producer};
use rskit_errors::{AppError, AppResult, ErrorCode};
use tracing::debug;

use crate::config::KafkaConfig;
use crate::event::Event;
use crate::message::Message;
use crate::traits::{EventProducer, MessageProducer};

/// Kafka-backed message producer wrapping an `rdkafka` `FutureProducer`.
pub struct KafkaProducer {
    producer: FutureProducer,
}

impl KafkaProducer {
    /// Create a new `KafkaProducer` from the given configuration.
    pub fn new(config: &KafkaConfig) -> AppResult<Self> {
        let producer: FutureProducer = config.to_client_config().create().map_err(|e| {
            AppError::new(
                ErrorCode::Internal,
                format!("Failed to create Kafka producer: {e}"),
            )
        })?;
        Ok(Self { producer })
    }
}

#[async_trait]
impl MessageProducer<Vec<u8>> for KafkaProducer {
    async fn send(&self, msg: Message<Vec<u8>>) -> AppResult<()> {
        let mut record = FutureRecord::to(&msg.topic).payload(&msg.payload);
        let key_ref;
        if let Some(ref key) = msg.key {
            key_ref = key.clone();
            record = record.key(&key_ref);
        }

        self.producer
            .send(record, Duration::from_secs(5))
            .await
            .map_err(|(e, _)| {
                AppError::new(
                    ErrorCode::ExternalService,
                    format!("Kafka send failed: {e}"),
                )
            })?;

        debug!(topic = %msg.topic, "message sent to Kafka");
        Ok(())
    }

    async fn send_batch(&self, msgs: Vec<Message<Vec<u8>>>) -> AppResult<()> {
        for msg in msgs {
            self.send(msg).await?;
        }
        Ok(())
    }

    async fn flush(&self, timeout: Duration) -> AppResult<()> {
        self.producer.flush(timeout).map_err(|e| {
            AppError::new(
                ErrorCode::ExternalService,
                format!("Kafka flush failed: {e}"),
            )
        })
    }
}

#[async_trait]
impl EventProducer for KafkaProducer {
    async fn publish(&self, topic: &str, event: Event) -> AppResult<()> {
        let payload = event.to_json()?;
        let msg = Message::new(topic, payload).with_key(&event.id);
        self.send(msg).await
    }

    async fn publish_batch(&self, topic: &str, events: Vec<Event>) -> AppResult<()> {
        for event in events {
            self.publish(topic, event).await?;
        }
        Ok(())
    }
}
