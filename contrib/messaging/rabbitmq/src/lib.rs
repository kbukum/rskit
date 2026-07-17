//! `RabbitMQ` adapter for `rskit-messaging`.
//!
//! The adapter uses AMQP queues named by message topic by default. Registration
//! is explicit and side-effect free; network connections are opened lazily.

#![warn(missing_docs)]

mod config;
mod connection;
mod consumer;
mod error;
mod producer;
mod register;
#[cfg(test)]
mod tests;

pub use config::RabbitMqConfig as Config;
pub use register::register;

#[cfg(test)]
pub(crate) use config::{queue_for, validate_name};
#[cfg(test)]
pub(crate) use consumer::{RabbitMqConsumer, shutdown_consumer_tasks, spawn_forwarding_task};
#[cfg(test)]
pub(crate) use error::{
    channel_close_failed, channel_failed, connect_failed, connect_timed_out,
    connection_close_failed, consume_failed, publish_confirm_failed, publish_failed,
    qos_configuration_failed, queue_declare_failed,
};
#[cfg(test)]
pub(crate) use producer::RabbitMqProducer;
#[cfg(test)]
pub(crate) use register::RabbitMqFactory;
