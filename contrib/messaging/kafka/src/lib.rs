//! Kafka adapter for `rskit-messaging`.
//!
//! Registration is explicit and side-effect free:
//! call [`register`](fn@register) from application composition code to add Kafka factories to a [`MessagingRegistry`](rskit_messaging::MessagingRegistry).

#![warn(missing_docs)]

mod client_config;
mod config;
mod consumer;
mod error;
mod producer;
mod register;
#[cfg(test)]
mod tests;

pub use config::{Compression, KafkaConfig as Config, OffsetReset, SecurityProtocol};
pub use register::register;

#[cfg(test)]
pub(crate) use client_config::{consumer_config, producer_config};
#[cfg(test)]
pub(crate) use consumer::{KafkaConsumer, recv_next_with_timeout};
#[cfg(test)]
pub(crate) use error::{
    kafka_consumer_creation_error, kafka_flush_error, kafka_producer_creation_error,
    kafka_receive_error, kafka_send_error, kafka_stream_ended_error, kafka_subscribe_error,
};
#[cfg(test)]
pub(crate) use producer::KafkaProducer;
#[cfg(test)]
pub(crate) use register::KafkaFactory;
