//! The [`FsChangeBatch`] value type: a debounce window's worth of changed paths.

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

/// A debounced batch of filesystem changes observed within one window.
///
/// Paths are deduplicated and kept in sorted order so a batch is deterministic
/// regardless of the order the underlying OS events arrived. Each path is
/// reported by the platform watcher as-is — typically matching the form of the
/// root it was registered under (so absolute roots yield absolute paths) — and
/// is *not* normalized or absolutized here; callers relativize or canonicalize
/// them against their own roots as needed.
///
/// A batch may additionally carry a **rescan** signal ([`rescan_requested`]):
/// the platform watcher reported an error (typically a queue overflow) during
/// the window, so some individual change notifications may have been dropped and
/// the reported [`paths`] are potentially incomplete. Consumers driving an
/// incremental rebuild should treat a rescan as "re-evaluate everything" rather
/// than trusting the path list alone. A rescan-only batch (overflow with no
/// surviving path events) has empty [`paths`] but is **not** [`is_empty`].
///
/// [`rescan_requested`]: Self::rescan_requested
/// [`paths`]: Self::paths
/// [`is_empty`]: Self::is_empty
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct FsChangeBatch {
    paths: BTreeSet<PathBuf>,
    rescan: bool,
}

impl FsChangeBatch {
    /// Build a batch from an already-deduplicated set of changed paths, with no
    /// rescan signal.
    #[must_use]
    pub const fn new(paths: BTreeSet<PathBuf>) -> Self {
        Self {
            paths,
            rescan: false,
        }
    }

    /// Set whether this batch requests a rescan (the watcher reported dropped
    /// events during the window, so [`paths`](Self::paths) may be incomplete).
    #[must_use]
    pub const fn with_rescan(mut self, rescan: bool) -> Self {
        self.rescan = rescan;
        self
    }

    /// The sorted, deduplicated changed paths in this batch.
    ///
    /// May be empty even when the batch is meaningful — see
    /// [`rescan_requested`](Self::rescan_requested).
    #[must_use]
    pub const fn paths(&self) -> &BTreeSet<PathBuf> {
        &self.paths
    }

    /// Whether the platform watcher dropped events during this window, so the
    /// reported [`paths`](Self::paths) may be incomplete and consumers should
    /// re-evaluate the watched tree from scratch.
    #[must_use]
    pub const fn rescan_requested(&self) -> bool {
        self.rescan
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

    /// Whether the batch carries no information — no changed paths and no rescan
    /// signal.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.paths.is_empty() && !self.rescan
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
        assert!(!FsChangeBatch::default().rescan_requested());
    }

    #[test]
    fn rescan_only_batch_is_not_empty() {
        let batch = FsChangeBatch::new(BTreeSet::new()).with_rescan(true);
        assert!(batch.rescan_requested());
        assert_eq!(batch.len(), 0);
        assert!(batch.paths().is_empty());
        assert!(!batch.is_empty(), "a rescan signal must not read as empty");
    }

    #[test]
    fn with_rescan_defaults_off() {
        let mut paths = BTreeSet::new();
        paths.insert(PathBuf::from("/repo/a.rs"));
        assert!(!FsChangeBatch::new(paths).rescan_requested());
    }
}
