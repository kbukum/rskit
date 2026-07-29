//! Composable git repository interfaces backed by libgit2 and the git CLI.
//!
//! This crate provides capability-based traits for git operations:
//! - [`Repository`] and [`Executor`] for core repository access and raw CLI execution
//! - [`Differ`], [`IgnoreReader`], [`TreeReader`], [`LogReader`], [`Blamer`],
//!   and [`Inspector`] for read flows
//! - [`IndexManager`], [`Committer`], and related write traits for mutating operations
//! - [`RefManager`], [`RemoteManager`], [`ConfigReader`], and [`Maintainer`] for management
//!
//! The default backend uses `git2` (libgit2) for embedded operations
//! and delegates several core mutating operations (merge, rebase, checkout, reset, stash, cherry-pick, maintenance) to the `git` CLI binary.
//! **A `git` binary must be available on `PATH` at runtime.** Network operations (fetch, push) use the embedded `git2` backend.
//!
//! # Usage
//!
//! ```no_run
//! use rskit_git::{Differ, Repository, open};
//!
//! let repo = open("/path/to/repo").unwrap();
//! let dirty = repo.is_dirty().unwrap();
//! let _status = repo.status().unwrap();
//! assert!(!repo.root().as_os_str().is_empty() || !dirty);
//! ```

#![warn(missing_docs)]

pub mod auth;
pub mod cli;
pub mod core;
pub mod embedded;
pub mod error;
pub mod manage;
pub mod options;
pub mod paths;
pub mod read;
pub mod repo;
#[cfg(test)]
mod tests;
#[cfg(feature = "testutil")]
pub mod testutil;
pub mod types;
pub mod write;

pub use auth::{
    AuthProvider, ChainAuthProvider, DefaultAuthProvider, EnvTokenAuthProvider, SigningConfig,
    StaticAuthProvider, TransportAuth,
};
pub use core::{Executor, Repository};
pub use error::GitError;
pub use manage::{ConfigReader, Maintainer, RefManager, RemoteManager};
pub use options::*;
pub use paths::{join_repo_path, repo_relative_path};
pub use read::{Blamer, Differ, IgnoreReader, IndexReader, Inspector, LogReader, TreeReader};
pub use repo::{
    Repo, clone, discover, discover_with_auth, init, init_bare, init_with, open, open_with_auth,
};
pub use rskit_errors::{AppError, AppResult};
pub use types::*;
pub use write::{
    CheckoutManager, CherryPicker, Committer, IndexManager, Merger, Rebaser, Resetter, Stasher,
};
