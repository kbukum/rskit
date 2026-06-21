use std::time::Duration;

use async_trait::async_trait;
use rskit_errors::{AppError, AppResult, ErrorCode};

use crate::event::Event;
use crate::message::Message;

fn unsupported(capability: &str) -> AppResult<()> {
    Err(AppError::new(
        ErrorCode::InvalidInput,
        format!("messaging capability {capability} is not supported by this consumer"),
    ))
}

// ── Messaging traits ─────────────────────────────────────────────────────────

/// A producer that sends messages to a broker.
#[async_trait]
pub trait MessageProducer<T: Send + Sync>: Send + Sync {
    /// Send a single message.
    async fn send(&self, msg: Message<T>) -> AppResult<()>;

    /// Send a batch of messages.
    async fn send_batch(&self, msgs: Vec<Message<T>>) -> AppResult<()>;

    /// Flush pending messages within the given timeout.
    async fn flush(&self, timeout: Duration) -> AppResult<()>;

    /// Close or shut down the producer. Implementations with no persistent resources may no-op.
    async fn close(&self) -> AppResult<()> {
        Ok(())
    }
}

/// A consumer that receives messages from a broker.
#[async_trait]
pub trait MessageConsumer<T: Send + Sync>: Send + Sync {
    /// Subscribe to one or more topics.
    async fn subscribe(&self, topics: &[&str]) -> AppResult<()>;

    /// Receive the next message. Blocks until a message is available.
    async fn recv(&self) -> AppResult<Message<T>>;

    /// Close or shut down the consumer. Implementations with no persistent resources may no-op.
    async fn close(&self) -> AppResult<()> {
        Ok(())
    }

    /// Pause message delivery when the adapter supports it.
    async fn pause(&self) -> AppResult<()> {
        unsupported("pause")
    }

    /// Resume paused message delivery when the adapter supports it.
    async fn resume(&self) -> AppResult<()> {
        unsupported("resume")
    }
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

// ── Broker lifecycle trait ───────────────────────────────────────────────────

/// Lifecycle management for a message-broker connection.
///
/// This is intentionally simpler than the bootstrap `Component` trait — it
/// captures the start / stop / health contract that every broker adapter
/// needs without pulling in the full component registry.  Implementations
/// can bridge to the bootstrap `Component` trait where needed.
#[async_trait]
pub trait BrokerComponent: Send + Sync {
    /// Establish the broker connection and perform any setup.
    async fn start(&self) -> AppResult<()>;
    /// Gracefully disconnect from the broker.
    async fn stop(&self) -> AppResult<()>;
    /// Instant health check — returns `true` when the broker connection is
    /// usable.
    fn is_healthy(&self) -> bool;
}
