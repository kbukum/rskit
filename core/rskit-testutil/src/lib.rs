//! Test utilities, mock providers, and assertion helpers.
//!
//! Designed for use in `#[cfg(test)]` blocks and integration tests across the rskit ecosystem.
//!
//! [`TestWorkspace`] is the generic fixture harness.
//! Configure any fixture root with [`TestWorkspace::with_fixture_dir`],
//! or use [`test_workspace!`] for the conventional `<crate>/tests/fixtures` layout.
//!
#![warn(missing_docs)]

/// Assertion helpers for `AppResult`.
pub mod assertions;
/// Fake components for lifecycle tests.
pub mod component;
/// Test config helpers.
pub mod config;
/// Process working-directory guard for tests.
pub mod current_dir;
/// Hook and event-bus test helpers.
pub mod hook;
/// Generic mock provider for testing.
pub mod mock_provider;
/// Temporary workspace and fixture helpers.
pub mod workspace;

pub use assertions::{assert_err_code, assert_ok};
pub use component::FakeComponent;
pub use config::TestAppConfig;
pub use current_dir::CurrentDirGuard;
pub use hook::TestEvent;
pub use mock_provider::MockProvider;
pub use workspace::TestWorkspace;

mod concurrency;
mod macros;

pub use concurrency::CONCURRENCY_TEST_NOTE;
