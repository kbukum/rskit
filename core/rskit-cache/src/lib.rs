//! Cache abstraction with an in-memory default and opt-in backend adapters.
//!
//! The core crate exports [`CacheBackend`], [`CacheRegistry`], [`MemoryCache`],
//! and [`TypedStore`]. External infrastructure backends live in `contrib/`
//! adapter crates and must be registered explicitly.
//!
//! No backend is registered by default; construct and inject a registry at the
//! composition boundary.

#![warn(missing_docs)]

/// Cache backend configuration and backend-specific options.
pub mod config;
/// In-memory cache backend.
pub mod memory;
/// Explicit backend registry and config-driven selection.
pub mod registry;
/// Generic JSON-serialised typed store backed by a [`CacheBackend`].
pub mod typed_store;

pub use config::{CacheConfig, MemoryConfig};
pub use memory::MemoryCache;
pub use registry::{CacheBackend, CacheFactory, CacheRegistry, register_memory};
pub use typed_store::TypedStore;
