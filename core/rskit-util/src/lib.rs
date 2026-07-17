//! Minimal domain-free utility crate for the rskit ecosystem.
//!
//! Provides fundamental helper modules for string casing, safe truncation,
//! collection transformation, safe environment variable parsing, duration/byte
//! size parsing, and stateless mathematical exponential backoff.
//!
//! # Modules
//!
//! - [`backoff`]: State-free mathematical backoff calculations with jitter.
//! - [`bytes`]: Formatting and parsing human-readable data sizes.
//! - [`collections`]: Vector grouping, chunking, indexing, and partition helpers.
//! - [`mod@env`]: Safe environment variable parsing with defaults.
//! - [`glob`]: Shell-style `*`/`?` matching over single string segments.
//! - [`hash`]: Content/interop hashing — BLAKE3 and SHA-256.
//! - [`secret`]: Prevent accidental credential leaks in logs/debug outputs.
//! - [`sensitive`]: Matching helpers for names that commonly carry secrets.
//! - [`strings`]: Casing, safe truncation, unique-shorthand resolution, and "did you mean?" suggestions.
//! - [`template`]: Lightweight template engine (`{name}` interpolation).
//! - [`time`]: Duration parsing, UTC date/time conversion, RFC 3339 helpers, and timing wrappers.

#![warn(missing_docs)]

pub mod backoff;
pub mod bytes;
pub mod collections;
mod decimal;
pub mod env;
pub mod glob;
pub mod hash;
pub mod secret;
pub mod sensitive;
pub mod strings;
pub mod template;
pub mod time;

pub(crate) use decimal::parse_decimal_scaled;
pub use secret::SecretString;
pub use sensitive::SecretKeyMatcher;
pub use template::{Placeholder, Template, TemplatePart};
