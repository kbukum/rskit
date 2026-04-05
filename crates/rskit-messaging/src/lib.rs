//! # rskit-messaging
//!
//! Message broker abstractions with an in-memory implementation for testing
//! and an optional Kafka backend (enable the `kafka` feature).
//!
//! ## Quick start
//!
//! ```rust
//! use rskit_messaging::memory::InMemoryBroker;
//! use rskit_messaging::{Message, MessageProducer, MessageConsumer};
//!
//! # #[tokio::main] async fn main() -> Result<(), Box<dyn std::error::Error>> {
//! let broker = InMemoryBroker::<String>::new(64);
//! let producer = broker.producer();
//! let consumer = broker.consumer();
//!
//! consumer.subscribe(&["events"]).await?;
//! producer.send(Message::new("events", "hello".into())).await?;
//!
//! let msg = consumer.recv().await?;
//! assert_eq!(msg.payload, "hello");
//! # Ok(())
//! # }
//! ```

pub mod batch;
pub mod bridge;
pub mod config;
pub mod errors;
pub mod event;
pub mod handler;
pub mod managed_consumer;
pub mod managed_producer;
pub mod memory;
pub mod message;
pub mod metrics;
pub mod middleware;
pub mod router;
pub mod runner;
pub mod traits;
pub mod translator;

#[cfg(feature = "kafka")]
pub mod kafka;

pub use batch::{BatchConfig, BatchProducer};
pub use config::{
    BrokerConfig, BrokerConfigExt, Compression, KafkaConfig, OffsetReset, SecurityProtocol,
};
pub use errors::{ErrorClassifier, NoopErrorClassifier};
pub use event::Event;
pub use handler::{FnHandler, HandlerMiddleware, MessageHandler, chain_handlers, middleware_fn};
pub use managed_consumer::{ManagedConsumer, ManagedConsumerBuilder};
pub use managed_producer::{ManagedProducer, ManagedProducerBuilder};
pub use memory::{
    InMemoryBroker, InMemoryConsumer, InMemoryProducer, assert_no_messages, assert_published,
    assert_published_n, wait_for_message,
};
pub use message::Message;
pub use metrics::{MetricsCollector, NoopMetrics};
pub use router::MessageRouter;
pub use runner::ConsumerRunner;
pub use traits::{BrokerComponent, EventConsumer, EventProducer, MessageConsumer, MessageProducer};
pub use translator::{JsonStringTranslator, JsonTranslator, MessageTranslator};
