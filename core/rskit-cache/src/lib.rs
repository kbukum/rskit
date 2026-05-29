//! Cache abstraction with local core adapters and opt-in remote adapters.
//!
//! The core crate exports [`CacheBackend`], [`CacheRegistry`], [`MemoryCache`],
//! and [`TypedStore`]. Local, infrastructure-free adapters live in this crate:
//! memory is always available, and filesystem storage is available with the
//! `fs` feature. Remote infrastructure backends live in `contrib/` adapter
//! crates and must be registered explicitly.
//!
//! No backend is registered by default; construct and inject a registry at the
//! composition boundary.

#![warn(missing_docs)]

/// Built-in cache adapters.
pub mod adapters;
/// Cache backend configuration and backend-specific options.
pub mod config;
/// Explicit backend registry and config-driven selection.
pub mod registry;
/// Generic JSON-serialised typed store backed by a [`CacheBackend`].
pub mod typed_store;

#[cfg(feature = "fs")]
pub use adapters::fs::{FileCache, FileCacheConfig, register_file_cache};
pub use adapters::memory::{MemoryCache, register_memory};
pub use config::{CacheConfig, MemoryConfig};
pub use registry::{CacheBackend, CacheFactory, CacheRegistry};
pub use typed_store::TypedStore;
