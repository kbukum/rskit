use std::collections::HashSet;
use std::time::Duration;

use async_trait::async_trait;
use lapin::options::BasicPublishOptions;
use lapin::{BasicProperties, Channel, Connection};
use rskit_errors::AppResult;
use rskit_messaging::{BrokerConfigExt, Event, EventProducer, Message, MessageProducer};
use tokio::sync::Mutex;
use tracing::debug;

use crate::Config;
use crate::config::{queue_for, validate_name};
use crate::connection::{connect, declare_queue};
use crate::error::{
    channel_close_failed, channel_failed, connection_close_failed, publish_confirm_failed,
    publish_failed,
};

/// RabbitMQ-backed message producer.
pub(crate) struct RabbitMqProducer {
    config: Config,
    state: Mutex<Option<RabbitMqProducerState>>,
    pub(crate) declared_queues: Mutex<HashSet<String>>,
}

struct RabbitMqProducerState {
    connection: Connection,
    channel: Channel,
}

impl RabbitMqProducer {
    /// Create a producer that connects lazily on send.
    pub(crate) fn new(config: Config) -> AppResult<Self> {
        config.validate()?;
        Ok(Self {
            config,
            state: Mutex::new(None),
            declared_queues: Mutex::new(HashSet::new()),
        })
    }

    async fn channel(&self) -> AppResult<Channel> {
        let mut guard = self.state.lock().await;
        if let Some(state) = guard.as_ref() {
            return Ok(state.channel.clone());
        }

        let connection = connect(&self.config).await?;
        let channel = connection.create_channel().await.map_err(channel_failed)?;
        *guard = Some(RabbitMqProducerState {
            connection,
            channel: channel.clone(),
        });
        drop(guard);
        Ok(channel)
    }

    pub(crate) async fn needs_queue_declare(&self, queue: &str) -> bool {
        self.config.declare_queues
            && self.config.exchange.is_empty()
            && !self.declared_queues.lock().await.contains(queue)
    }

    pub(crate) async fn mark_queue_declared(&self, queue: &str) {
        self.declared_queues.lock().await.insert(queue.to_string());
    }
}

#[async_trait]
impl MessageProducer<Vec<u8>> for RabbitMqProducer {
    async fn send(&self, msg: Message<Vec<u8>>) -> AppResult<()> {
        validate_name("RabbitMQ routing key", &msg.topic)?;
        let routing_key = queue_for(&self.config, &msg.topic)?;
        let channel = self.channel().await?;
        if self.needs_queue_declare(&routing_key).await {
            declare_queue(&channel, &routing_key, self.config.durable_queues).await?;
            self.mark_queue_declared(&routing_key).await;
        }
        channel
            .basic_publish(
                self.config.exchange.as_str().into(),
                routing_key.into(),
                BasicPublishOptions::default(),
                &msg.payload,
                BasicProperties::default(),
            )
            .await
            .map_err(publish_failed)?
            .await
            .map_err(publish_confirm_failed)?;
        debug!(topic = %msg.topic, "message sent to RabbitMQ");
        Ok(())
    }

    async fn send_batch(&self, msgs: Vec<Message<Vec<u8>>>) -> AppResult<()> {
        for msg in msgs {
            self.send(msg).await?;
        }
        Ok(())
    }

    async fn flush(&self, _timeout: Duration) -> AppResult<()> {
        Ok(())
    }

    async fn close(&self) -> AppResult<()> {
        let state = self.state.lock().await.take();
        if let Some(state) = state {
            state
                .channel
                .close(200, "closed".into())
                .await
                .map_err(channel_close_failed)?;
            state
                .connection
                .close(200, "closed".into())
                .await
                .map_err(connection_close_failed)?;
        }
        self.declared_queues.lock().await.clear();
        Ok(())
    }
}

#[async_trait]
impl EventProducer for RabbitMqProducer {
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
