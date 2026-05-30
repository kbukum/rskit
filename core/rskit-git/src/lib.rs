//! Composable git repository interfaces backed by libgit2 and the git CLI.
//!
//! This crate provides capability-based traits for git operations:
//! - [`Repository`] and [`Executor`] for core repository access and raw CLI execution
//! - [`Differ`], [`TreeReader`], [`LogReader`], [`Blamer`], and [`Inspector`] for read flows
//! - [`IndexManager`], [`Committer`], and related write traits for mutating operations
//! - [`RefManager`], [`RemoteManager`], [`ConfigReader`], and [`Maintainer`] for management
//!
//! The default backend uses `git2` (libgit2) for embedded operations and delegates several
//! core mutating operations (merge, rebase, checkout, reset, stash, cherry-pick, maintenance)
//! to the `git` CLI binary. **A `git` binary must be available on `PATH` at runtime.**
//! Network operations (fetch, push) use the embedded `git2` backend.
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
pub mod read;
pub mod repo;
#[cfg(feature = "testutil")]
pub mod testutil;
pub mod types;
pub mod write;

pub use core::{Executor, Repository};
pub use error::GitError;
pub use manage::{ConfigReader, Maintainer, RefManager, RemoteManager};
pub use options::*;
pub use read::{Blamer, Differ, IndexReader, Inspector, LogReader, TreeReader};
pub use repo::{Repo, clone, discover, init, init_bare, open};
pub use rskit_errors::{AppError, AppResult};
pub use types::*;
pub use write::{
    CheckoutManager, CherryPicker, Committer, IndexManager, Merger, Rebaser, Resetter, Stasher,
};
