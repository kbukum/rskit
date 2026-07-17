//! NATS adapter for `rskit-messaging`.
//!
//! Connections are established lazily by producer/consumer operations; explicit
//! registration itself has no network side effects.

#![warn(missing_docs)]

mod config;
mod connection;
mod consumer;
mod error;
mod producer;
mod register;
#[cfg(test)]
mod tests;

pub use config::NatsConfig as Config;
pub use register::register;

#[cfg(test)]
pub(crate) use config::subject_for;
#[cfg(test)]
pub(crate) use connection::connect_options;
#[cfg(test)]
pub(crate) use consumer::{NatsConsumer, shutdown_consumer_tasks, spawn_forwarding_task};
#[cfg(test)]
pub(crate) use error::{
    nats_close_flush_error, nats_connect_error, nats_flush_error, nats_publish_error,
    nats_subscribe_error,
};
#[cfg(test)]
pub(crate) use producer::NatsProducer;
#[cfg(test)]
pub(crate) use register::NatsFactory;
