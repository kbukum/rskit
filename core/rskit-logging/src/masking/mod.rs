//! Sensitive data masking engine for structured log output.
//!
//! Provides a [`Masker`] trait (adapter pattern)
//! and a [`DefaultMasker`] implementation that redacts common secrets, PII,
//! and credentials from log output before it reaches any sink.
//!
//! Masking is **on by default** — callers must explicitly disable it via [`MaskingConfig::enabled`].
//!
//! # Examples
//!
//! ```rust
//! use rskit_logging::masking::{DefaultMasker, Masker, MaskingConfig};
//!
//! let masker = DefaultMasker::default();
//! assert_eq!(masker.mask_value("password", "hunter2"), "[REDACTED]");
//! assert_eq!(masker.mask_value("name", "Alice"), "Alice");
//! ```

mod config;
mod default_masker;
mod masker;
mod writer;

pub use config::MaskingConfig;
pub use default_masker::{DefaultMasker, mask_value};
pub use masker::Masker;
pub use writer::{MaskingMakeWriter, MaskingWriter};

#[cfg(test)]
mod tests;
