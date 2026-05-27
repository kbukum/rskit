//! Synchronous filesystem operations.
//!
//! These APIs are `std::fs`-backed and may block the current thread.

/// Sync directory helpers.
pub mod dir;
/// Sync file helpers.
pub mod file;
/// Sync file tree helpers.
pub mod tree;
