//! Redis adapter for [`rskit_cache`].

#![warn(missing_docs)]

mod client;
mod config;
mod registration;

pub(crate) use client::RedisClient;
pub use config::Config;
pub use registration::register;

#[cfg(test)]
mod tests;

#[cfg(test)]
pub(crate) use client::{prefixed_key, redis_err, redis_ttl_millis};
#[cfg(test)]
pub(crate) use registration::RedisFactory;
