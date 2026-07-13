//! # rskit-messaging
//!
//! Message broker abstractions with an in-memory implementation for testing.
//! Broker SDKs live in opt-in adapter crates such as `rskit-messaging-kafka`.
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
//! let msg = consumer.recv(std::time::Duration::from_secs(1)).await?;
//! assert_eq!(msg.payload, "hello");
//! # Ok(())
//! # }
//! ```

pub mod batch;
pub mod bridge;
/// Broker configuration types and policy enums.
pub mod config;
pub mod errors;
pub mod event;
pub mod event_publisher;
pub mod handler;
pub mod managed_consumer;
pub mod managed_producer;
pub mod memory;
/// Message envelope and metadata types.
pub mod message;
pub mod metrics;
pub mod middleware;
pub mod registry;
pub mod router;
pub mod runner;
/// Core producer, consumer, and broker traits.
pub mod traits;
pub mod translator;

pub use batch::{BatchConfig, BatchProducer};
pub use config::{
    BrokerConfig, BrokerConfigExt, BrokerConfigOverrides, CommitStrategy, DeliveryGuarantee,
    DlqPolicy,
};
pub use errors::{ErrorClassifier, NoopErrorClassifier};
pub use event::Event;
pub use event_publisher::EventPublisher;
pub use handler::{FnHandler, HandlerMiddleware, MessageHandler, chain_handlers, middleware_fn};
pub use managed_consumer::{ManagedConsumer, ManagedConsumerBuilder};
pub use managed_producer::{ManagedProducer, ManagedProducerBuilder};
pub use memory::{
    InMemoryBroker, InMemoryConsumer, InMemoryProducer, assert_no_messages, assert_published,
    assert_published_n, wait_for_message,
};
pub use message::Message;
pub use metrics::{MetricsCollector, NoopMetrics};
pub use middleware::StackBuilder;
pub use registry::{MessagingBackend, MessagingFactory, MessagingRegistry};
pub use router::MessageRouter;
pub use runner::ConsumerRunner;
pub use traits::{BrokerComponent, EventConsumer, EventProducer, MessageConsumer, MessageProducer};
pub use translator::{JsonStringTranslator, JsonTranslator, MessageTranslator};
