//! File tree types.

use std::path::PathBuf;

/// A recursive file tree entry.
#[derive(Debug, Clone)]
pub struct TreeEntry {
    /// Absolute or caller-provided path to the entry.
    pub path: PathBuf,
    /// Path relative to the listed root.
    pub relative_path: PathBuf,
    /// Whether the entry is a regular file.
    pub is_file: bool,
    /// Whether the entry is a directory.
    pub is_dir: bool,
    /// Whether the entry is a symlink.
    pub is_symlink: bool,
}

/// Options for copying a directory tree.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CopyTreeOptions {
    /// Replace existing destination files.
    pub overwrite: bool,
    /// Follow symlinks instead of skipping them. Defaults to `false` for safety.
    pub follow_symlinks: bool,
}

impl Default for CopyTreeOptions {
    fn default() -> Self {
        Self {
            overwrite: true,
            follow_symlinks: false,
        }
    }
}
