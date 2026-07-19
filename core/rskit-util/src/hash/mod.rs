//! Content and interop hashing helpers.
//!
//! - BLAKE3 content hashing ([`ContentHasher`], [`hash_hex`]) for stable cache keys, change detection,
//!   and deduplication.
//! - SHA-256 digests ([`sha256`]) for wire-format and interop use cases.

mod content;
pub use content::{ContentHasher, hash_hex};

/// SHA-256 digests for wire-format and interop use cases.
pub mod sha256;
