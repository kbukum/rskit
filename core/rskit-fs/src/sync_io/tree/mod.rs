//! File tree helpers.
//!
//! These helpers use blocking `std::fs` I/O. When calling them from async contexts,
//! run them through `tokio::task::spawn_blocking` or an equivalent blocking executor boundary.

mod copy;
mod ignore_walk;
mod list;
mod remove;
#[cfg(test)]
mod tests;
mod traversal;
mod types;
mod walk;

pub use copy::copy_tree;
pub use ignore_walk::{IgnoreWalkOptions, walk_tree_ignoring};
pub use list::list_tree;
pub use remove::{remove_tree, remove_tree_if_exists};
pub use types::{CopyTreeOptions, TreeEntry, WalkControl, WalkEntryFilter, WalkOptions};
pub use walk::walk_tree;

pub(crate) use traversal::{
    VisitedDirs, canonical_dir, ensure_directory, enter_directory, init_visited_dirs, metadata_for,
};
