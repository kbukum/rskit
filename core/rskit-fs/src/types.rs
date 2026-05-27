//! Shared filesystem metadata types.

use std::ffi::OsString;
use std::path::PathBuf;

/// Metadata for an entry directly inside a directory.
#[derive(Debug, Clone)]
pub struct DirEntry {
    /// Entry path.
    pub path: PathBuf,
    /// Entry file name.
    pub file_name: OsString,
    /// Whether the entry is a regular file.
    pub is_file: bool,
    /// Whether the entry is a directory.
    pub is_dir: bool,
    /// Whether the entry is a symlink.
    pub is_symlink: bool,
}

/// Metadata for a filesystem entry at a file path.
#[derive(Debug, Clone)]
pub struct FileMeta {
    /// File path.
    pub path: PathBuf,
    /// File size in bytes.
    pub len: u64,
    /// Creation time, when available.
    pub created: Option<std::time::SystemTime>,
    /// Last modification time, when available.
    pub modified: Option<std::time::SystemTime>,
    /// Whether this path is a regular file.
    pub is_file: bool,
    /// Whether this path is a directory.
    pub is_dir: bool,
    /// Whether this path is a symlink.
    pub is_symlink: bool,
}
