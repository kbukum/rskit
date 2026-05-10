//! Read-oriented git traits.

use rskit_errors::AppResult;

use crate::options::{BlameOptions, DescribeOptions, GrepOptions, LogOptions};
use crate::types::{
    BlameLine, Commit, DiffEntry, DiffStats, GrepMatch, Oid, StatusEntry, TreeEntry, TreeHash,
};

/// Diff and working tree status operations.
pub trait Differ {
    /// Returns file changes between two refs.
    fn diff(&self, from: &str, to: &str) -> AppResult<Vec<DiffEntry>>;

    /// Returns aggregated statistics for changes between two refs.
    fn diff_stats(&self, from: &str, to: &str) -> AppResult<DiffStats>;

    /// Returns the working tree status.
    fn status(&self) -> AppResult<Vec<StatusEntry>>;
}

/// Read access to git tree objects.
pub trait TreeReader {
    /// Returns the OID of the tree at the given path and revision.
    fn tree_hash(&self, revision: &str, path: &str) -> AppResult<TreeHash>;

    /// Returns the content of a file at the given revision and path.
    fn file_at(&self, revision: &str, path: &str) -> AppResult<Vec<u8>>;

    /// Returns the entries in a tree at the given revision and path.
    fn list_entries(&self, revision: &str, path: &str) -> AppResult<Vec<TreeEntry>>;
}

/// Read-only access to commit history and graph queries.
pub trait LogReader {
    /// Returns commits reachable from `HEAD`, filtered by the provided options.
    fn log(&self, opts: Option<&LogOptions>) -> AppResult<Vec<Commit>>;

    /// Returns the merge base between two revisions.
    fn merge_base(&self, a: &str, b: &str) -> AppResult<Oid>;

    /// Reports whether `ancestor` is an ancestor of `descendant`.
    fn is_ancestor(&self, ancestor: &str, descendant: &str) -> AppResult<bool>;
}

/// Read-only access to `git blame` data.
pub trait Blamer {
    /// Returns per-line blame information for a file at the given revision.
    fn blame(
        &self,
        revision: &str,
        path: &str,
        opts: Option<&BlameOptions>,
    ) -> AppResult<Vec<BlameLine>>;
}

/// Read-only advanced inspection helpers.
pub trait Inspector {
    /// Describes the current revision in a human-readable form.
    fn describe(&self, opts: Option<&DescribeOptions>) -> AppResult<String>;

    /// Resolves a revision expression to an object ID.
    fn rev_parse(&self, revision: &str) -> AppResult<Oid>;

    /// Searches tracked content for pattern matches.
    fn grep(
        &self,
        pattern: &str,
        revision: &str,
        opts: Option<&GrepOptions>,
    ) -> AppResult<Vec<GrepMatch>>;

    /// Returns raw object content for a revision expression.
    fn show(&self, object: &str) -> AppResult<Vec<u8>>;
}
