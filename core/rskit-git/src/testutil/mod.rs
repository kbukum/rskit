//! Test utilities for the git module.
//!
//! Provides a [`RepoBuilder`] for creating temporary git repositories with specific states for testing.

mod builder;

pub use builder::RepoBuilder;
