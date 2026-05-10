//! Write-oriented git traits.

use rskit_errors::AppResult;

use crate::options::{
    CheckoutOptions, CherryPickOptions, CommitOptions, MergeOptions, RebaseOptions,
};
use crate::types::{MergeResult, Oid, RebaseResult, ResetMode, StashEntry, StatusEntry};

/// Stage and unstage repository paths through the git index.
pub trait IndexManager {
    /// Stages the provided repository-relative paths.
    fn stage(&self, paths: &[&str]) -> AppResult<()>;

    /// Removes the provided repository-relative paths from the index.
    fn unstage(&self, paths: &[&str]) -> AppResult<()>;

    /// Lists entries that currently differ between HEAD and the index.
    fn staged_entries(&self) -> AppResult<Vec<StatusEntry>>;
}

/// Creates new commits from the repository index.
pub trait Committer {
    /// Creates a commit from the current index state and returns its object ID.
    fn commit(&self, message: &str, opts: Option<&CommitOptions>) -> AppResult<Oid>;
}

/// Merge operations.
pub trait Merger {
    /// Merges a branch into the current HEAD.
    fn merge(&self, branch: &str, opts: Option<&MergeOptions>) -> AppResult<MergeResult>;

    /// Aborts an in-progress merge.
    fn abort_merge(&self) -> AppResult<()>;

    /// Aborts an in-progress merge.
    fn merge_abort(&self) -> AppResult<()> {
        self.abort_merge()
    }
}

/// Rebase operations.
pub trait Rebaser {
    /// Rebases the current branch onto another target.
    fn rebase(&self, onto: &str, opts: Option<&RebaseOptions>) -> AppResult<RebaseResult>;

    /// Aborts an in-progress rebase.
    fn abort_rebase(&self) -> AppResult<()>;

    /// Continues an in-progress rebase.
    fn continue_rebase(&self) -> AppResult<RebaseResult>;

    /// Aborts an in-progress rebase.
    fn rebase_abort(&self) -> AppResult<()> {
        self.abort_rebase()
    }

    /// Continues an in-progress rebase.
    fn rebase_continue(&self) -> AppResult<RebaseResult> {
        self.continue_rebase()
    }
}

/// Cherry-pick operations.
pub trait CherryPicker {
    /// Cherry-picks the given commit.
    fn cherry_pick(&self, commit: &str, opts: Option<&CherryPickOptions>) -> AppResult<Oid>;

    /// Continues an in-progress cherry-pick.
    fn cherry_pick_continue(&self) -> AppResult<Oid>;

    /// Aborts an in-progress cherry-pick.
    fn cherry_pick_abort(&self) -> AppResult<()>;
}

/// Reset operations.
pub trait Resetter {
    /// Resets repository state to the target revision.
    fn reset(&self, target: &str, mode: ResetMode) -> AppResult<()>;
}

/// Checkout operations.
pub trait Checker {
    /// Checks out the given ref.
    fn checkout(&self, ref_name: &str, opts: Option<&CheckoutOptions>) -> AppResult<()>;

    /// Checks out the provided paths from the index or HEAD.
    fn checkout_files(&self, paths: &[&str]) -> AppResult<()>;
}

/// Stash operations.
pub trait Stasher {
    /// Creates a stash entry.
    fn stash(&self, message: &str) -> AppResult<Oid>;

    /// Creates a stash entry.
    fn stash_push(&self, message: &str) -> AppResult<Oid> {
        self.stash(message)
    }

    /// Applies and drops the top stash entry.
    fn stash_pop(&self) -> AppResult<()>;

    /// Applies and drops the selected stash entry.
    fn stash_pop_index(&self, index: usize) -> AppResult<()>;

    /// Lists stash entries.
    fn stash_list(&self) -> AppResult<Vec<StashEntry>>;
}
