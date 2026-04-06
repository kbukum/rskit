use async_trait::async_trait;
use rdkafka::Message as RdMessage;
use rdkafka::consumer::{Consumer, StreamConsumer};
use rskit_errors::{AppError, AppResult, ErrorCode};
use tokio_stream::StreamExt;
use tracing::debug;

use crate::config::KafkaConfig;
use crate::event::Event;
use crate::message::Message;
use crate::traits::{EventConsumer, MessageConsumer};

/// Kafka-backed message consumer wrapping an `rdkafka` `StreamConsumer`.
pub struct KafkaConsumer {
    consumer: StreamConsumer,
}

impl KafkaConsumer {
    /// Create a new `KafkaConsumer` from the given configuration.
    pub fn new(config: &KafkaConfig) -> AppResult<Self> {
        let consumer: StreamConsumer = config.to_consumer_config().create().map_err(|e| {
            AppError::new(
                ErrorCode::Internal,
                format!("Failed to create Kafka consumer: {e}"),
            )
        })?;
        Ok(Self { consumer })
    }
}

#[async_trait]
impl MessageConsumer<Vec<u8>> for KafkaConsumer {
    async fn subscribe(&self, topics: &[&str]) -> AppResult<()> {
        self.consumer.subscribe(topics).map_err(|e| {
            AppError::new(
                ErrorCode::ExternalService,
                format!("Kafka subscribe failed: {e}"),
            )
        })
    }

    async fn recv(&self) -> AppResult<Message<Vec<u8>>> {
        let mut stream = self.consumer.stream();
        let borrowed = stream.next().await.ok_or_else(|| {
            AppError::new(
                ErrorCode::ExternalService,
                "Kafka stream ended unexpectedly",
            )
        })?;

        let borrowed = borrowed.map_err(|e| {
            AppError::new(
                ErrorCode::ExternalService,
                format!("Kafka receive error: {e}"),
            )
        })?;

        let topic = borrowed.topic().to_string();
        let payload = borrowed.payload().map(|p| p.to_vec()).unwrap_or_default();
        let key = borrowed
            .key()
            .map(|k| String::from_utf8_lossy(k).to_string());
        let partition = Some(borrowed.partition());
        let offset = Some(borrowed.offset());

        let mut msg = Message::new(topic, payload);
        if let Some(k) = key {
            msg = msg.with_key(k);
        }
        msg.partition = partition;
        msg.offset = offset;

        debug!(
            topic = %msg.topic,
            partition = ?msg.partition,
            offset = ?msg.offset,
            "message received from Kafka"
        );

        Ok(msg)
    }
}

#[async_trait]
impl EventConsumer for KafkaConsumer {
    async fn subscribe(&self, topics: &[&str]) -> AppResult<()> {
        MessageConsumer::subscribe(self, topics).await
    }

    async fn recv_event(&self) -> AppResult<Event> {
        let msg = MessageConsumer::recv(self).await?;
        Event::from_json(&msg.payload)
    }
}
