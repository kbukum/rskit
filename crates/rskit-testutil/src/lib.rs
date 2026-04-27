//! Test utilities, mock providers, and assertion helpers.
//!
//! Designed for use in `#[cfg(test)]` blocks and integration tests across
//! the rskit ecosystem.
//!
// NOTE(#31): New async tests should use #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
// to catch concurrency bugs that only manifest with real parallelism.

#![warn(missing_docs)]

/// Assertion helpers for `AppResult`.
pub mod assertions;
/// Generic mock provider for testing.
pub mod mock_provider;

pub use assertions::{assert_err_code, assert_ok};
pub use mock_provider::MockProvider;
