use std::collections::HashMap;

use chrono::{DateTime, Utc};
use uuid::Uuid;

/// A generic message that can be sent to or received from a message broker.
#[derive(Debug, Clone)]
pub struct Message<T> {
    /// The topic or channel the message belongs to.
    pub topic: String,
    /// Optional partitioning key.
    pub key: Option<String>,
    /// The message payload.
    pub payload: T,
    /// Arbitrary key-value headers attached to the message.
    pub headers: HashMap<String, String>,
    /// Timestamp when the message was created.
    pub timestamp: DateTime<Utc>,
    /// The partition the message was sent to or read from.
    pub partition: Option<i32>,
    /// The offset within the partition.
    pub offset: Option<i64>,
}

impl<T> Message<T> {
    /// Create a new message for the given topic with the provided payload.
    pub fn new(topic: impl Into<String>, payload: T) -> Self {
        Self {
            topic: topic.into(),
            key: None,
            payload,
            headers: HashMap::new(),
            timestamp: Utc::now(),
            partition: None,
            offset: None,
        }
    }

    /// Set the partitioning key.
    pub fn with_key(mut self, key: impl Into<String>) -> Self {
        self.key = Some(key.into());
        self
    }

    /// Attach a single header.
    pub fn with_header(mut self, key: impl Into<String>, value: impl Into<String>) -> Self {
        self.headers.insert(key.into(), value.into());
        self
    }

    /// Attach a unique message-id header.
    pub fn with_message_id(self) -> Self {
        self.with_header("message-id", Uuid::new_v4().to_string())
    }
}
