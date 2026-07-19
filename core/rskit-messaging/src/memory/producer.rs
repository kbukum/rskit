//! In-memory message and event producer.

use std::sync::Arc;
use std::time::Duration;

use async_trait::async_trait;
use rskit_errors::{AppError, AppResult, ErrorCode};

use super::state::PublishState;
use crate::event::Event;
use crate::message::Message;
use crate::traits::{EventProducer, MessageProducer};

/// An in-memory message producer.
#[derive(Debug, Clone)]
pub struct InMemoryProducer<T: Clone + Send + Sync + 'static> {
    pub(super) state: Arc<PublishState<T>>,
}

#[async_trait]
impl<T: Clone + Send + Sync + 'static> MessageProducer<T> for InMemoryProducer<T> {
    async fn send(&self, msg: Message<T>) -> AppResult<()> {
        self.state.publish(msg).await
    }

    async fn send_batch(&self, msgs: Vec<Message<T>>) -> AppResult<()> {
        for msg in msgs {
            self.send(msg).await?;
        }
        Ok(())
    }

    async fn flush(&self, _timeout: Duration) -> AppResult<()> {
        // In-memory delivery is instant; nothing to flush.
        Ok(())
    }
}

#[async_trait]
impl EventProducer for InMemoryProducer<serde_json::Value> {
    async fn publish(&self, topic: &str, event: Event) -> AppResult<()> {
        let value = serde_json::to_value(&event).map_err(|e| {
            AppError::new(
                ErrorCode::Internal,
                format!("Failed to serialize event: {e}"),
            )
        })?;
        self.send(Message::new(topic, value)).await
    }

    async fn publish_batch(&self, topic: &str, events: Vec<Event>) -> AppResult<()> {
        for event in events {
            self.publish(topic, event).await?;
        }
        Ok(())
    }
}
