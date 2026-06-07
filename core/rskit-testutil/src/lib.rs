//! Test utilities, mock providers, and assertion helpers.
//!
//! Designed for use in `#[cfg(test)]` blocks and integration tests across
//! the rskit ecosystem.
//!
//! [`TestWorkspace`] is the generic fixture harness. Configure any fixture root
//! with [`TestWorkspace::with_fixture_dir`], or use [`test_workspace!`] for the
//! conventional `<crate>/tests/fixtures` layout.
//!
#![warn(missing_docs)]

/// Assertion helpers for `AppResult`.
pub mod assertions;
/// Fake components for lifecycle tests.
pub mod component;
/// Test config helpers.
pub mod config;
/// Hook and event-bus test helpers.
pub mod hook;
/// Generic mock provider for testing.
pub mod mock_provider;
/// Temporary workspace and fixture helpers.
pub mod workspace;

pub use assertions::{assert_err_code, assert_ok};
pub use component::FakeComponent;
pub use config::TestAppConfig;
pub use hook::TestEvent;
pub use mock_provider::MockProvider;
pub use workspace::TestWorkspace;

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
