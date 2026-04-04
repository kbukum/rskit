//! Kafka backend for message production and consumption.
//!
//! Requires the `kafka` feature flag.

mod consumer;
mod producer;

pub use consumer::KafkaConsumer;
pub use producer::KafkaProducer;
