//! The [`FsChangeBatch`] value type: a debounce window's worth of changed paths.

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

/// A debounced batch of filesystem paths that changed within one window.
///
/// Paths are deduplicated and kept in sorted order so a batch is deterministic
/// regardless of the order the underlying OS events arrived. Each path is
/// reported by the platform watcher as-is — typically matching the form of the
/// root it was registered under (so absolute roots yield absolute paths) — and
/// is *not* normalized or absolutized here; callers relativize or canonicalize
/// them against their own roots as needed.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct FsChangeBatch {
    paths: BTreeSet<PathBuf>,
}

impl FsChangeBatch {
    /// Build a batch from an already-deduplicated set of changed paths.
    #[must_use]
    pub const fn new(paths: BTreeSet<PathBuf>) -> Self {
        Self { paths }
    }

    /// The sorted, deduplicated changed paths in this batch.
    #[must_use]
    pub const fn paths(&self) -> &BTreeSet<PathBuf> {
        &self.paths
    }

    /// Whether any path in the batch matches `predicate`.
    pub fn any(&self, predicate: impl Fn(&Path) -> bool) -> bool {
        self.paths.iter().any(|path| predicate(path))
    }

    /// The number of distinct changed paths.
    #[must_use]
    pub fn len(&self) -> usize {
        self.paths.len()
    }

    /// Whether the batch carries no paths.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.paths.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeSet;
    use std::path::{Path, PathBuf};

    use super::FsChangeBatch;

    #[test]
    fn batch_reports_sorted_deduplicated_paths() {
        let mut paths = BTreeSet::new();
        paths.insert(PathBuf::from("/repo/b.rs"));
        paths.insert(PathBuf::from("/repo/a.rs"));
        let batch = FsChangeBatch::new(paths);

        let collected: Vec<&Path> = batch.paths().iter().map(PathBuf::as_path).collect();
        assert_eq!(
            collected,
            [Path::new("/repo/a.rs"), Path::new("/repo/b.rs")]
        );
        assert_eq!(batch.len(), 2);
        assert!(!batch.is_empty());
    }

    #[test]
    fn any_matches_a_contained_path() {
        let mut paths = BTreeSet::new();
        paths.insert(PathBuf::from("/repo/src/lib.rs"));
        let batch = FsChangeBatch::new(paths);

        assert!(batch.any(|path| path.ends_with("lib.rs")));
        assert!(!batch.any(|path| path.ends_with("main.rs")));
    }

    #[test]
    fn default_batch_is_empty() {
        assert!(FsChangeBatch::default().is_empty());
    }
}
