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
//! - tree copy/list operations do not follow symlinks unless explicitly requested;
//! - use [`file::write_atomic`] for replacing file contents without exposing partial writes;
//! - use [`permissions`] capability checks before performing optional user-facing operations.

#![warn(missing_docs)]

/// Directory helpers.
pub mod dir;
/// File helpers.
pub mod file;
/// Link helpers.
pub mod link;
/// Safe path helpers.
pub mod path;
/// Permission and capability helpers.
pub mod permissions;
/// Temporary file and path helpers.
pub mod temp;
/// File tree helpers.
pub mod tree;

pub use path::{
    SafePathError, absolute, canonicalize, parent_dir, safe_join, validate_relative_path,
};
pub use temp::{TempDir, TempFile, sibling_temp_path};
pub use tree::{
    CopyTreeOptions, TreeEntry, WalkControl, WalkEntryFilter, WalkOptions, copy_tree, list_tree,
    walk_tree,
};
