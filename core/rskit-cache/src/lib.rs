//! Cache abstraction with local core stores and opt-in remote stores.
//!
//! The core crate exports [`CacheStore`], [`CacheRegistry`], [`MemoryCache`], and [`TypedStore`].
//! Local, infrastructure-free adapters live in this crate: memory is always available,
//! and filesystem storage is available with the `fs` feature.
//! Remote infrastructure stores live in `contrib/` adapter crates and must be registered explicitly.
//!
//! No store is registered by default; construct and inject a registry at the composition boundary.

#![warn(missing_docs)]

/// Built-in cache adapters.
pub mod adapters;
/// Cache store configuration and store-specific options.
pub mod config;
/// Explicit store registry and config-driven selection.
pub mod registry;
/// Generic JSON-serialised typed store backed by a [`CacheStore`].
pub mod typed_store;

/// Compatibility exports for the original in-memory adapter module path.
pub mod memory;

#[cfg(feature = "fs")]
pub use adapters::fs::{FileCache, FileCacheConfig, register_file_cache};
pub use adapters::memory::{MemoryCache, register_memory};
pub use config::{CacheConfig, MemoryConfig};
pub use registry::{CacheRegistry, CacheStore, CacheStoreFactory};
pub use typed_store::TypedStore;
