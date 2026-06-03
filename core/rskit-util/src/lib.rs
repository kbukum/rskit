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
//! - [`secret`]: Prevent accidental credential leaks in logs/debug outputs.
//! - [`strings`]: Zero-alloc/low-alloc casing and safe truncation.
//! - [`template`]: Lightweight template engine (`{name}` interpolation).
//! - [`time`]: Duration string parsing, formatting, and `time_it` timing wrappers.

#![warn(missing_docs)]

pub mod backoff;
pub mod bytes;
pub mod collections;
pub mod env;
pub mod secret;
pub mod strings;
pub mod template;
pub mod time;

pub use secret::SecretString;
pub use template::{Placeholder, Template, TemplatePart};
