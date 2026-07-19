//! In-memory message broker, producer, and consumer for testing.

mod assertions;
mod broker;
mod consumer;
mod factory;
mod producer;
mod state;

#[cfg(test)]
mod tests;

pub use assertions::{assert_no_messages, assert_published, assert_published_n, wait_for_message};
pub use broker::InMemoryBroker;
pub use consumer::InMemoryConsumer;
pub use factory::register;
pub use producer::InMemoryProducer;
