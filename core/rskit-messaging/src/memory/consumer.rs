//! In-memory message and event consumer.

use std::collections::HashSet;
use std::sync::Arc;

use async_trait::async_trait;
use rskit_errors::{AppError, AppResult, ErrorCode};
use tokio::sync::{Mutex, broadcast};

use crate::event::Event;
use crate::message::Message;
use crate::traits::{EventConsumer, MessageConsumer};

/// An in-memory message consumer.
#[derive(Debug)]
pub struct InMemoryConsumer<T: Clone + Send + Sync + 'static> {
    rx: Arc<Mutex<broadcast::Receiver<Message<T>>>>,
    topics: Arc<Mutex<HashSet<String>>>,
}

impl<T: Clone + Send + Sync + 'static> InMemoryConsumer<T> {
    /// Create a consumer over a fresh broadcast receiver with no topic filter.
    pub(super) fn new(rx: broadcast::Receiver<Message<T>>) -> Self {
        Self {
            rx: Arc::new(Mutex::new(rx)),
            topics: Arc::new(Mutex::new(HashSet::new())),
        }
    }
}

// Manual Clone because broadcast::Receiver is not Clone but we can resubscribe.
impl<T: Clone + Send + Sync + 'static> Clone for InMemoryConsumer<T> {
    fn clone(&self) -> Self {
        Self {
            rx: self.rx.clone(),
            topics: self.topics.clone(),
        }
    }
}

#[async_trait]
impl<T: Clone + Send + Sync + 'static> MessageConsumer<T> for InMemoryConsumer<T> {
    async fn subscribe(&self, topics: &[&str]) -> AppResult<()> {
        {
            let mut set = self.topics.lock().await;
            for t in topics {
                set.insert((*t).to_string());
            }
        }
        Ok(())
    }

    async fn recv(&self, timeout: std::time::Duration) -> AppResult<Message<T>> {
        if timeout.is_zero() {
            return Err(AppError::new(
                ErrorCode::InvalidInput,
                "message receive timeout must be greater than zero",
            ));
        }
        tokio::time::timeout(timeout, async {
            loop {
                let msg = {
                    let mut rx = self.rx.lock().await;
                    rx.recv().await.map_err(|e| {
                        AppError::new(ErrorCode::ExternalService, format!("receive failed: {e}"))
                    })?
                };

                let topics = self.topics.lock().await;
                // If no explicit subscription, accept all messages.
                if topics.is_empty() || topics.contains(&msg.topic) {
                    return Ok(msg);
                }
                // Otherwise loop to skip messages for other topics.
            }
        })
        .await
        .map_err(|error| AppError::timeout("message receive").with_cause(error))?
    }
}

#[async_trait]
impl EventConsumer for InMemoryConsumer<serde_json::Value> {
    async fn subscribe(&self, topics: &[&str]) -> AppResult<()> {
        MessageConsumer::subscribe(self, topics).await
    }

    async fn recv_event(&self, timeout: std::time::Duration) -> AppResult<Event> {
        let msg = self.recv(timeout).await?;
        serde_json::from_value(msg.payload).map_err(|e| {
            AppError::new(
                ErrorCode::Internal,
                format!("Failed to deserialize event: {e}"),
            )
        })
    }
}
