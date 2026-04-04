use std::time::Duration;

use async_trait::async_trait;
use rskit_errors::AppResult;

use crate::event::Event;
use crate::message::Message;

/// A producer that sends messages to a broker.
#[async_trait]
pub trait MessageProducer<T: Send + Sync>: Send + Sync {
    /// Send a single message.
    async fn send(&self, msg: Message<T>) -> AppResult<()>;

    /// Send a batch of messages.
    async fn send_batch(&self, msgs: Vec<Message<T>>) -> AppResult<()>;

    /// Flush pending messages within the given timeout.
    async fn flush(&self, timeout: Duration) -> AppResult<()>;
}

/// A consumer that receives messages from a broker.
#[async_trait]
pub trait MessageConsumer<T: Send + Sync>: Send + Sync {
    /// Subscribe to one or more topics.
    async fn subscribe(&self, topics: &[&str]) -> AppResult<()>;

    /// Receive the next message. Blocks until a message is available.
    async fn recv(&self) -> AppResult<Message<T>>;
}

/// A producer that publishes structured [`Event`]s to topics.
#[async_trait]
pub trait EventProducer: Send + Sync {
    /// Publish a single event to the given topic.
    async fn publish(&self, topic: &str, event: Event) -> AppResult<()>;

    /// Publish a batch of events to the given topic.
    async fn publish_batch(&self, topic: &str, events: Vec<Event>) -> AppResult<()>;
}

/// A consumer that receives structured [`Event`]s from topics.
#[async_trait]
pub trait EventConsumer: Send + Sync {
    /// Subscribe to one or more topics.
    async fn subscribe(&self, topics: &[&str]) -> AppResult<()>;

    /// Receive the next event. Blocks until an event is available.
    async fn recv_event(&self) -> AppResult<Event>;
}
