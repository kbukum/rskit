//! Redis client with typed store, connection management, and Component lifecycle.
//!
//! # Overview
//!
//! `rskit-cache` provides an async Redis client ([`RedisClient`]) backed by
//! [`redis::aio::ConnectionManager`] and a generic JSON-serialised store
//! ([`TypedStore`]) for strongly-typed caching.
//!
//! The client implements the [`rskit_bootstrap::Component`] trait, making it
//! easy to integrate into the rskit application lifecycle.

/// Redis connection and pool configuration.
pub mod config;
/// Async Redis client with string, hash, list, scan, and pub/sub operations.
pub mod client;
/// Generic JSON-serialised typed store backed by [`RedisClient`].
pub mod typed_store;

pub use client::RedisClient;
pub use config::RedisConfig;
pub use typed_store::TypedStore;
