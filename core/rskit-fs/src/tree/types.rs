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

/// Options for walking a directory tree.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct WalkOptions {
    /// Follow symlinks instead of visiting them as link entries.
    pub follow_symlinks: bool,
    /// Which entry kinds are passed to callbacks.
    pub entry_filter: WalkEntryFilter,
}

impl Default for WalkOptions {
    fn default() -> Self {
        Self {
            follow_symlinks: false,
            entry_filter: WalkEntryFilter::ALL,
        }
    }
}

/// Entry kinds included during a tree walk.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct WalkEntryFilter(u8);

impl WalkEntryFilter {
    /// Include regular files.
    pub const FILES: Self = Self(0b001);
    /// Include directories.
    pub const DIRS: Self = Self(0b010);
    /// Include symlinks.
    pub const SYMLINKS: Self = Self(0b100);
    /// Include regular files and directories.
    pub const FILES_AND_DIRS: Self = Self(Self::FILES.0 | Self::DIRS.0);
    /// Include every entry kind.
    pub const ALL: Self = Self(Self::FILES.0 | Self::DIRS.0 | Self::SYMLINKS.0);

    /// Return true when regular files are included.
    #[must_use]
    pub const fn includes_files(self) -> bool {
        self.0 & Self::FILES.0 != 0
    }

    /// Return true when directories are included.
    #[must_use]
    pub const fn includes_dirs(self) -> bool {
        self.0 & Self::DIRS.0 != 0
    }

    /// Return true when symlinks are included.
    #[must_use]
    pub const fn includes_symlinks(self) -> bool {
        self.0 & Self::SYMLINKS.0 != 0
    }
}

/// Control returned by a tree-walk callback.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum WalkControl {
    /// Continue walking normally.
    Continue,
    /// Skip the current directory subtree.
    SkipSubtree,
    /// Stop walking immediately.
    Stop,
}
