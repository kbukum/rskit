//! Test utilities, mock providers, and assertion helpers.
//!
//! Designed for use in `#[cfg(test)]` blocks and integration tests across
//! the rskit ecosystem.
//!
// NOTE(#31): New async tests should use #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
// to catch concurrency bugs that only manifest with real parallelism.

// NOTE(#31): New async tests should use #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
// to catch concurrency bugs that only manifest with real parallelism.

#![warn(missing_docs)]

/// Assertion helpers for `AppResult`.
pub mod assertions;
/// Generic mock provider for testing.
pub mod mock_provider;

pub use assertions::{assert_err_code, assert_ok};
pub use mock_provider::MockProvider;

/// Use `#[tokio::test(flavor = "multi_thread", worker_threads = 2)]` for
/// tests that exercise concurrent code paths. Plain `#[tokio::test]` uses
/// a single-threaded runtime and will NOT catch data races.
///
/// See issue #65 for context.
pub const CONCURRENCY_TEST_NOTE: &str =
    "Use #[tokio::test(flavor = \"multi_thread\")] for concurrency tests";

/// Assert that a `Result` is `Ok`, printing the error on failure.
#[macro_export]
macro_rules! assert_ok {
    ($result:expr) => {
        match $result {
            Ok(v) => v,
            Err(e) => panic!("expected Ok, got Err: {:?}", e),
        }
    };
}

/// Assert that a `Result` is `Err`.
#[macro_export]
macro_rules! assert_err {
    ($result:expr) => {
        match $result {
            Err(e) => e,
            Ok(v) => panic!("expected Err, got Ok: {:?}", v),
        }
    };
}
