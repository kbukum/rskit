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

pub mod config;
pub mod event;
pub mod memory;
pub mod message;
pub mod traits;

#[cfg(feature = "kafka")]
pub mod kafka;

pub use config::{Compression, KafkaConfig, OffsetReset, SecurityProtocol};
pub use event::Event;
pub use memory::{InMemoryBroker, InMemoryConsumer, InMemoryProducer};
pub use message::Message;
pub use traits::{EventConsumer, EventProducer, MessageConsumer, MessageProducer};
