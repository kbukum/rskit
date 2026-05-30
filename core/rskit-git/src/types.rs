//! Shared types for git operations.

use std::fmt;
use std::time::SystemTime;

/// Git object ID (SHA-1 hash, 20 bytes).
#[derive(Clone, Copy, PartialEq, Eq, Hash)]
pub struct Oid([u8; 20]);

impl Oid {
    /// Creates an OID from raw bytes.
    pub fn from_bytes(bytes: [u8; 20]) -> Self {
        Self(bytes)
    }

    /// Returns the raw bytes.
    pub fn as_bytes(&self) -> &[u8; 20] {
        &self.0
    }

    /// Reports whether this is the zero OID.
    pub fn is_zero(&self) -> bool {
        self.0 == [0u8; 20]
    }
}

impl fmt::Display for Oid {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        for byte in &self.0 {
            write!(f, "{byte:02x}")?;
        }
        Ok(())
    }
}

impl fmt::Debug for Oid {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "Oid({self})")
    }
}

/// OID of a tree object for content-addressed comparison.
pub type TreeHash = Oid;

/// A git reference (branch, tag, or HEAD).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Reference {
    /// Fully qualified or symbolic reference name.
    pub name: String,
    /// Target object ID.
    pub target: Oid,
    /// Whether this reference is a branch.
    pub is_branch: bool,
    /// Whether this reference is a tag.
    pub is_tag: bool,
}

/// Author or committer identity.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Signature {
    /// Display name.
    pub name: String,
    /// Email address.
    pub email: String,
    /// Timestamp of the signature.
    pub when: SystemTime,
}

/// A git commit object.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Commit {
    /// Commit object ID.
    pub oid: Oid,
    /// Author identity.
    pub author: Signature,
    /// Committer identity.
    pub committer: Signature,
    /// Commit message.
    pub message: String,
    /// Parent commit IDs.
    pub parents: Vec<Oid>,
}

/// How a file changed in a diff.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum FileStatus {
    /// File was added.
    Added,
    /// File content changed.
    Modified,
    /// File was removed.
    Deleted,
    /// File was renamed.
    Renamed,
    /// File was copied.
    Copied,
    /// File is untracked.
    Untracked,
    /// File is ignored.
    Ignored,
    /// File type changed.
    TypeChanged,
    /// File has merge conflicts.
    Conflicted,
}

impl fmt::Display for FileStatus {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Added => write!(f, "added"),
            Self::Modified => write!(f, "modified"),
            Self::Deleted => write!(f, "deleted"),
            Self::Renamed => write!(f, "renamed"),
            Self::Copied => write!(f, "copied"),
            Self::Untracked => write!(f, "untracked"),
            Self::Ignored => write!(f, "ignored"),
            Self::TypeChanged => write!(f, "type_changed"),
            Self::Conflicted => write!(f, "conflicted"),
        }
    }
}

/// A single file change between two refs.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DiffEntry {
    /// Path of the changed file in the new tree.
    pub path: String,
    /// Previous path when renamed or copied.
    pub old_path: Option<String>,
    /// Previous object ID.
    pub old_oid: Oid,
    /// New object ID.
    pub new_oid: Oid,
    /// Kind of change.
    pub status: FileStatus,
}

/// Aggregated diff statistics.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct DiffStats {
    /// Lines added.
    pub additions: usize,
    /// Lines deleted.
    pub deletions: usize,
    /// Number of changed files.
    pub files_changed: usize,
}

/// A file's state in the working tree or index.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum EntryState {
    /// Changes staged in the index.
    Staged,
    /// Changes present only in the working tree.
    Unstaged,
    /// Path not tracked by git.
    Untracked,
    /// Path has merge conflicts.
    Conflicted,
}

impl fmt::Display for EntryState {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Staged => write!(f, "staged"),
            Self::Unstaged => write!(f, "unstaged"),
            Self::Untracked => write!(f, "untracked"),
            Self::Conflicted => write!(f, "conflicted"),
        }
    }
}

/// A file's status in the working tree.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StatusEntry {
    /// Repository-relative file path.
    pub path: String,
    /// Current working tree or index state.
    pub state: EntryState,
}

/// A file entry in the git index.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct IndexEntry {
    /// Repository-relative file path.
    pub path: String,
    /// Object ID stored in the index.
    pub oid: Oid,
    /// Entry kind.
    pub kind: EntryKind,
    /// Raw git file mode.
    pub filemode: u32,
}

/// Type of a tree entry.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum EntryKind {
    /// Regular file/blob entry.
    Blob,
    /// Nested tree/directory entry.
    Tree,
    /// Git submodule entry.
    Submodule,
}

impl fmt::Display for EntryKind {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Blob => write!(f, "blob"),
            Self::Tree => write!(f, "tree"),
            Self::Submodule => write!(f, "submodule"),
        }
    }
}

/// An entry within a git tree object.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TreeEntry {
    /// Entry name relative to its parent tree.
    pub name: String,
    /// Object ID of the entry.
    pub oid: Oid,
    /// Entry kind.
    pub kind: EntryKind,
    /// Raw git file mode.
    pub filemode: u32,
}

/// Branch metadata.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Branch {
    /// Branch name.
    pub name: String,
    /// Tip commit ID.
    pub target: Oid,
    /// Upstream tracking branch (for example `origin/main`).
    pub upstream: Option<String>,
}

/// Tag metadata.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Tag {
    /// Tag name.
    pub name: String,
    /// Target object ID.
    pub target: Oid,
    /// Tagger signature (`None` for lightweight tags).
    pub tagger: Option<Signature>,
    /// Annotation message (`""` for lightweight tags).
    pub message: String,
}

/// Remote repository metadata.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Remote {
    /// Remote name.
    pub name: String,
    /// Remote URL.
    pub url: String,
    /// Fetch refspecs.
    pub fetch_specs: Vec<String>,
    /// Push refspecs.
    pub push_specs: Vec<String>,
}

/// Line-level attribution from `git blame`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BlameLine {
    /// One-based line number.
    pub line: usize,
    /// Commit that last changed the line.
    pub commit_oid: Oid,
    /// Author of the blamed line.
    pub author: Signature,
    /// Full line content.
    pub content: String,
}

/// A match returned from `git grep`-style repository inspection.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GrepMatch {
    /// Repository-relative file path.
    pub path: String,
    /// One-based line number, or `None` when line numbers were not requested.
    pub line_number: Option<usize>,
    /// Raw matching line content.
    pub line: String,
}

/// Information about a stash entry.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StashEntry {
    /// Zero-based stash index.
    pub index: usize,
    /// Stash commit OID when known.
    pub oid: Oid,
    /// Human-readable stash message.
    pub message: String,
}

/// Result returned from merge operations.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct MergeResult {
    /// The resulting HEAD OID when available.
    pub head: Option<Oid>,
    /// Whether the merge completed as a fast-forward.
    pub fast_forward: bool,
    /// Conflicting paths produced by the merge.
    pub conflicts: Vec<String>,
}

/// Result returned from rebase operations.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct RebaseResult {
    /// The resulting HEAD OID when available.
    pub head: Option<Oid>,
    /// Number of commits applied during the rebase.
    pub applied: usize,
    /// Conflicting paths encountered during the rebase.
    pub conflicts: Vec<String>,
}

/// Controls which branches to list.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
#[non_exhaustive]
pub enum BranchFilter {
    /// Only local branches.
    #[default]
    Local,
    /// Only remote branches.
    Remote,
    /// Both local and remote branches.
    All,
}

/// Controls repository reset behavior.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
#[non_exhaustive]
pub enum ResetMode {
    /// Reset HEAD and index, preserving worktree changes.
    #[default]
    Mixed,
    /// Reset HEAD only.
    Soft,
    /// Reset HEAD, index, and worktree.
    Hard,
}
