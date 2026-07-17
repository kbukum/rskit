use std::time::Duration;

use async_trait::async_trait;
use rdkafka::producer::{FutureProducer, FutureRecord, Producer};
use rskit_errors::{AppError, AppResult, ErrorCode};
use rskit_messaging::{BrokerConfigExt, Event, EventProducer, Message, MessageProducer};
use tracing::debug;

use crate::Config;
use crate::client_config::producer_config;
use crate::config::validate_topic;
use crate::error::{kafka_flush_error, kafka_producer_creation_error, kafka_send_error};

/// Wall-clock bound applied to each message delivery so producer sends never
/// block indefinitely on a stalled broker.
const SEND_TIMEOUT: Duration = Duration::from_secs(5);

/// Kafka-backed message producer wrapping an `rdkafka` `FutureProducer`.
pub(crate) struct KafkaProducer {
    producer: FutureProducer,
}

impl KafkaProducer {
    /// Create a new `KafkaProducer` from the given configuration.
    pub(crate) fn new(config: &Config) -> AppResult<Self> {
        config.validate()?;
        let producer: FutureProducer = producer_config(config)
            .create()
            .map_err(|error| kafka_producer_creation_error(&error))?;
        Ok(Self { producer })
    }
}

#[async_trait]
impl MessageProducer<Vec<u8>> for KafkaProducer {
    async fn send(&self, msg: Message<Vec<u8>>) -> AppResult<()> {
        validate_topic("Kafka topic", &msg.topic)?;
        let mut record = FutureRecord::to(&msg.topic).payload(&msg.payload);
        let key_ref;
        if let Some(ref key) = msg.key {
            key_ref = key.clone();
            record = record.key(&key_ref);
        }

        self.producer
            .send(record, SEND_TIMEOUT)
            .await
            .map_err(|(error, _)| kafka_send_error(&error))?;

        debug!(topic = %msg.topic, "message sent to Kafka");
        Ok(())
    }

    async fn send_batch(&self, msgs: Vec<Message<Vec<u8>>>) -> AppResult<()> {
        for msg in &msgs {
            validate_topic("Kafka topic", &msg.topic)?;
        }
        let mut deliveries = Vec::with_capacity(msgs.len());
        for msg in &msgs {
            let mut record = FutureRecord::to(&msg.topic).payload(&msg.payload);
            if let Some(key) = msg.key.as_deref() {
                record = record.key(key);
            }
            let delivery = self
                .producer
                .send_result(record)
                .map_err(|(error, _)| kafka_send_error(&error))?;
            deliveries.push(delivery);
        }
        for delivery in deliveries {
            tokio::time::timeout(SEND_TIMEOUT, delivery)
                .await
                .map_err(|error| AppError::timeout("Kafka delivery").with_cause(error))?
                .map_err(|error| {
                    AppError::new(
                        ErrorCode::ExternalService,
                        "Kafka delivery future was cancelled",
                    )
                    .with_cause(error)
                })?
                .map_err(|(error, _)| kafka_send_error(&error))?;
        }
        Ok(())
    }

    async fn flush(&self, timeout: Duration) -> AppResult<()> {
        self.producer
            .flush(timeout)
            .map_err(|error| kafka_flush_error(&error))
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
        validate_topic("Kafka topic", topic)?;
        let mut msgs = Vec::with_capacity(events.len());
        for event in events {
            let payload = event.to_json()?;
            msgs.push(Message::new(topic, payload).with_key(&event.id));
        }
        self.send_batch(msgs).await
    }
}
