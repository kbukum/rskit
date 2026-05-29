//! Built-in cache adapters.

/// In-memory cache adapter.
pub mod memory;

/// Filesystem cache adapter.
#[cfg(feature = "fs")]
pub mod fs;
