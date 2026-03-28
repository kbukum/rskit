use std::time::Duration;

use async_trait::async_trait;
use rskit_errors::AppResult;

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
