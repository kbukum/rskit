use std::future::Future;
use std::time::Duration;

use async_trait::async_trait;
use rdkafka::Message as RdMessage;
use rdkafka::consumer::{Consumer, StreamConsumer};
use rskit_errors::{AppError, AppResult, ErrorCode};
use rskit_messaging::{BrokerConfigExt, Event, EventConsumer, Message, MessageConsumer};
use tokio_stream::StreamExt;
use tracing::debug;

use crate::Config;
use crate::client_config::consumer_config;
use crate::config::validate_topic;
use crate::error::{
    kafka_consumer_creation_error, kafka_receive_error, kafka_stream_ended_error,
    kafka_subscribe_error,
};

pub(crate) async fn recv_next_with_timeout<F, T>(future: F, timeout: Duration) -> AppResult<T>
where
    F: Future<Output = T>,
{
    tokio::time::timeout(timeout, future)
        .await
        .map_err(|error| AppError::timeout("Kafka receive").with_cause(error))
}

/// Kafka-backed message consumer wrapping an `rdkafka` `StreamConsumer`.
pub(crate) struct KafkaConsumer {
    consumer: StreamConsumer,
}

impl KafkaConsumer {
    /// Create a new `KafkaConsumer` from the given configuration.
    pub(crate) fn new(config: &Config) -> AppResult<Self> {
        config.validate()?;
        let consumer: StreamConsumer = consumer_config(config)
            .create()
            .map_err(|error| kafka_consumer_creation_error(&error))?;
        Ok(Self { consumer })
    }
}

#[async_trait]
impl MessageConsumer<Vec<u8>> for KafkaConsumer {
    async fn subscribe(&self, topics: &[&str]) -> AppResult<()> {
        for topic in topics {
            validate_topic("Kafka topic", topic)?;
        }
        self.consumer
            .subscribe(topics)
            .map_err(|error| kafka_subscribe_error(&error))
    }

    async fn recv(&self, timeout: Duration) -> AppResult<Message<Vec<u8>>> {
        if timeout.is_zero() {
            return Err(AppError::new(
                ErrorCode::InvalidInput,
                "Kafka receive timeout must be greater than zero",
            ));
        }
        let mut stream = self.consumer.stream();
        let borrowed = recv_next_with_timeout(stream.next(), timeout)
            .await?
            .ok_or_else(kafka_stream_ended_error)?;

        let borrowed = borrowed.map_err(|error| kafka_receive_error(&error))?;

        let mut msg = Message::new(
            borrowed.topic().to_string(),
            borrowed.payload().map_or_else(Vec::new, ToOwned::to_owned),
        );
        if let Some(key) = borrowed.key() {
            msg = msg.with_key(String::from_utf8_lossy(key).to_string());
        }

        msg.partition = Some(borrowed.partition());
        msg.offset = Some(borrowed.offset());

        debug!(topic = %msg.topic, partition = ?msg.partition, offset = ?msg.offset, "message received from Kafka");
        Ok(msg)
    }
}

#[async_trait]
impl EventConsumer for KafkaConsumer {
    async fn subscribe(&self, topics: &[&str]) -> AppResult<()> {
        MessageConsumer::subscribe(self, topics).await
    }

    async fn recv_event(&self, timeout: Duration) -> AppResult<Event> {
        let msg = MessageConsumer::recv(self, timeout).await?;
        Event::from_json(&msg.payload)
    }
}
