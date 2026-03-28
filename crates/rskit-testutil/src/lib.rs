//! Test utilities, mock providers, and assertion helpers.
//!
//! Designed for use in `#[cfg(test)]` blocks and integration tests across
//! the rskit ecosystem.

#![warn(missing_docs)]

/// Assertion helpers for `AppResult`.
pub mod assertions;
/// Generic mock provider for testing.
pub mod mock_provider;

pub use assertions::{assert_err_code, assert_ok};
pub use mock_provider::MockProvider;
