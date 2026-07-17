use std::time::Duration;

use async_nats::Client;
use async_trait::async_trait;
use bytes::Bytes;
use rskit_errors::AppResult;
use rskit_messaging::{BrokerConfigExt, Event, EventProducer, Message, MessageProducer};
use tokio::sync::Mutex;
use tracing::debug;

use crate::Config;
use crate::config::{subject_for, validate_subject};
use crate::connection::connect;
use crate::error::{
    nats_close_flush_error, nats_connect_error, nats_flush_error, nats_publish_error,
};

/// NATS-backed message producer.
pub(crate) struct NatsProducer {
    config: Config,
    client: Mutex<Option<Client>>,
}

impl NatsProducer {
    /// Create a producer that connects lazily on first use.
    pub(crate) fn new(config: Config) -> AppResult<Self> {
        config.validate()?;
        Ok(Self {
            config,
            client: Mutex::new(None),
        })
    }

    async fn client(&self) -> AppResult<Client> {
        let mut guard = self.client.lock().await;
        if let Some(client) = guard.as_ref() {
            return Ok(client.clone());
        }
        let client = connect(&self.config).await.map_err(nats_connect_error)?;
        *guard = Some(client.clone());
        drop(guard);
        Ok(client)
    }
}

#[async_trait]
impl MessageProducer<Vec<u8>> for NatsProducer {
    async fn send(&self, msg: Message<Vec<u8>>) -> AppResult<()> {
        validate_subject("NATS subject", &msg.topic)?;
        let subject = subject_for(&self.config, &msg.topic)?;
        let client = self.client().await?;
        client
            .publish(subject, Bytes::from(msg.payload))
            .await
            .map_err(nats_publish_error)?;
        debug!(topic = %msg.topic, "message sent to NATS");
        Ok(())
    }

    async fn send_batch(&self, msgs: Vec<Message<Vec<u8>>>) -> AppResult<()> {
        for msg in msgs {
            self.send(msg).await?;
        }
        Ok(())
    }

    async fn flush(&self, _timeout: Duration) -> AppResult<()> {
        let client = self.client().await?;
        client.flush().await.map_err(nats_flush_error)
    }

    async fn close(&self) -> AppResult<()> {
        let client = self.client.lock().await.take();
        if let Some(client) = client {
            client.flush().await.map_err(nats_close_flush_error)?;
        }
        Ok(())
    }
}

#[async_trait]
impl EventProducer for NatsProducer {
    async fn publish(&self, topic: &str, event: Event) -> AppResult<()> {
        self.send(Message::new(topic, event.to_json()?)).await
    }

    async fn publish_batch(&self, topic: &str, events: Vec<Event>) -> AppResult<()> {
        for event in events {
            self.publish(topic, event).await?;
        }
        Ok(())
    }
}
