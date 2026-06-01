//! Local filesystem primitives for paths, files, directories, links, permissions,
//! temporary files, and file trees.
//!
//! This crate intentionally stays below storage abstractions. Higher-level
//! crates such as `rskit-storage`, `rskit-cache`, and `rskit-httpclient` use
//! these primitives instead of each reimplementing path safety, temp files, and
//! atomic file replacement.
//!
//! Security defaults:
//! - use [`path::safe_join`] for user-provided relative paths before touching disk;
//! - sync tree copy/list operations do not follow symlinks unless explicitly requested;
//! - use `async_io::file::write_atomic` or `sync_io::file::write_atomic` for same-filesystem writes
//!   without exposing partial files, and `write_atomic_replace` when existing files should be
//!   replaced (existing-file replacement is atomic on Unix-like platforms; Windows replacement
//!   removes the destination before renaming because the platform rename operation cannot replace
//!   an existing file);
//! - use [`permissions`] capability checks before performing optional user-facing operations when
//!   the `async` feature is enabled.

#![warn(missing_docs)]

/// Platform application directory helpers.
pub mod app_dirs;
/// Async filesystem operations.
#[cfg(feature = "async")]
pub mod async_io;
/// Async hard-link and symbolic-link helpers.
#[cfg(feature = "async")]
pub mod link;
/// Safe path helpers.
pub mod path;
/// Permission and capability helpers.
#[cfg(feature = "async")]
pub mod permissions;
/// Synchronous filesystem operations.
pub mod sync_io;
/// Temporary file and path helpers.
pub mod temp;
mod types;

pub use app_dirs::app_cache_dir;
pub use path::{
    SafePathError, absolute, canonicalize, parent_dir, safe_join, validate_relative_path,
};
pub use temp::{TempDir, TempFile, sibling_temp_path};
pub use types::{DirEntry, FileMeta};
