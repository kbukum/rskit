//! Repository orchestrator that delegates to libgit2 and Git CLI implementations.

use std::path::Path;
use std::sync::Arc;

use rskit_errors::AppResult;

use crate::auth::AuthProvider;
use crate::cli;
use crate::core::{Executor, Repository};
use crate::embedded;
use crate::manage::{ConfigReader, Maintainer, RefManager, RemoteManager};
use crate::options::{
    BlameOptions, CheckoutOptions, CleanOptions, CommitOptions, DescribeOptions, FetchOptions,
    GrepOptions, InitOptions, LogOptions, MergeOptions, PushOptions, RebaseOptions, SignOptions,
};
use crate::read::{Blamer, Differ, IgnoreReader, IndexReader, Inspector, LogReader, TreeReader};
use crate::types::{
    BlameLine, Branch, BranchFilter, Commit, DiffEntry, DiffStats, GrepMatch, IndexEntry,
    MergeResult, Oid, RebaseResult, Reference, Remote, ResetMode, StashEntry, StatusEntry, Tag,
    TreeEntry, TreeHash,
};
use crate::write::{
    CheckoutManager, CherryPicker, Committer, IndexManager, Merger, Rebaser, Resetter, Stasher,
};

/// Repository facade that combines embedded and CLI capabilities.
pub struct Repo {
    embedded: embedded::Git2Repository,
    cli: cli::GitCli,
}

impl Repo {
    fn new(embedded: embedded::Git2Repository) -> Self {
        let cli = cli::GitCli::new(embedded.root().to_path_buf());
        Self { embedded, cli }
    }
}

/// Opens a git repository at the given path (canonicalized).
pub fn open(path: impl AsRef<Path>) -> AppResult<Repo> {
    embedded::open(path).map(Repo::new)
}

/// Discovers a git repository by walking up from the given path.
pub fn discover(path: impl AsRef<Path>) -> AppResult<Repo> {
    embedded::discover(path).map(Repo::new)
}

/// Opens a git repository at the given path with an explicit auth provider.
///
/// The provider supplies transport credentials (for example a token read from
/// caller-named environment variables) to network operations such as push and
/// fetch on the embedded backend.
pub fn open_with_auth(path: impl AsRef<Path>, auth: Arc<dyn AuthProvider>) -> AppResult<Repo> {
    embedded::open_with_auth(path, auth).map(Repo::new)
}

/// Discovers a git repository by walking up from `path` with an explicit auth provider.
pub fn discover_with_auth(path: impl AsRef<Path>, auth: Arc<dyn AuthProvider>) -> AppResult<Repo> {
    embedded::discover_with_auth(path, auth).map(Repo::new)
}

/// Clones a repository into the given path.
pub fn clone(url: &str, path: impl AsRef<Path>) -> AppResult<Repo> {
    embedded::clone(url, path).map(Repo::new)
}

/// Initializes a new git repository at the given path.
///
/// The initial branch is [`DEFAULT_BRANCH`](crate::DEFAULT_BRANCH) regardless
/// of the host's Git configuration; use [`init_with`] to choose another name.
pub fn init(path: impl AsRef<Path>) -> AppResult<Repo> {
    embedded::init(path).map(Repo::new)
}

/// Initializes a new git repository at the given path with explicit options.
pub fn init_with(path: impl AsRef<Path>, options: &InitOptions) -> AppResult<Repo> {
    embedded::init_with(path, options).map(Repo::new)
}

/// Initializes a new bare git repository at the given path.
///
/// The initial branch is [`DEFAULT_BRANCH`](crate::DEFAULT_BRANCH), matching [`init`].
pub fn init_bare(path: impl AsRef<Path>) -> AppResult<Repo> {
    embedded::init_bare(path).map(Repo::new)
}

impl Repository for Repo {
    fn root(&self) -> &Path {
        self.embedded.root()
    }

    fn head(&self) -> AppResult<Reference> {
        self.embedded.head()
    }

    fn resolve_ref(&self, refname: &str) -> AppResult<Oid> {
        self.embedded.resolve_ref(refname)
    }

    fn is_dirty(&self) -> AppResult<bool> {
        self.embedded.is_dirty()
    }
}

impl Executor for Repo {
    fn exec(&self, args: &[&str]) -> AppResult<Vec<u8>> {
        self.cli.exec(args)
    }
}

impl Differ for Repo {
    fn diff(&self, from: &str, to: &str) -> AppResult<Vec<DiffEntry>> {
        self.embedded.diff(from, to)
    }

    fn diff_stats(&self, from: &str, to: &str) -> AppResult<DiffStats> {
        self.embedded.diff_stats(from, to)
    }

    fn status(&self) -> AppResult<Vec<StatusEntry>> {
        self.embedded.status()
    }
}

impl IgnoreReader for Repo {
    fn is_ignored(&self, path: &str) -> AppResult<bool> {
        self.embedded.is_ignored(path)
    }
}

impl TreeReader for Repo {
    fn tree_hash(&self, revision: &str, path: &str) -> AppResult<TreeHash> {
        self.embedded.tree_hash(revision, path)
    }

    fn file_at(&self, revision: &str, path: &str) -> AppResult<Vec<u8>> {
        self.embedded.file_at(revision, path)
    }

    fn file_at_bounded(&self, revision: &str, path: &str, max_bytes: u64) -> AppResult<Vec<u8>> {
        self.embedded.file_at_bounded(revision, path, max_bytes)
    }

    fn list_entries(&self, revision: &str, path: &str) -> AppResult<Vec<TreeEntry>> {
        self.embedded.list_entries(revision, path)
    }
}

impl IndexReader for Repo {
    fn index_entry(&self, path: &str) -> AppResult<Option<IndexEntry>> {
        self.embedded.index_entry(path)
    }
}

impl LogReader for Repo {
    fn log(&self, opts: Option<&LogOptions>) -> AppResult<Vec<Commit>> {
        self.embedded.log(opts)
    }

    fn merge_base(&self, a: &str, b: &str) -> AppResult<Oid> {
        self.embedded.merge_base(a, b)
    }

    fn is_ancestor(&self, ancestor: &str, descendant: &str) -> AppResult<bool> {
        self.embedded.is_ancestor(ancestor, descendant)
    }
}

impl Blamer for Repo {
    fn blame(
        &self,
        revision: &str,
        path: &str,
        opts: Option<&BlameOptions>,
    ) -> AppResult<Vec<BlameLine>> {
        self.embedded.blame(revision, path, opts)
    }
}

impl Inspector for Repo {
    fn describe(&self, opts: Option<&DescribeOptions>) -> AppResult<String> {
        self.cli.describe(opts)
    }

    fn rev_parse(&self, revision: &str) -> AppResult<Oid> {
        self.cli.rev_parse(revision)
    }

    fn grep(
        &self,
        pattern: &str,
        revision: &str,
        opts: Option<&GrepOptions>,
    ) -> AppResult<Vec<GrepMatch>> {
        self.cli.grep(pattern, revision, opts)
    }

    fn show(&self, object: &str) -> AppResult<Vec<u8>> {
        self.cli.show(object)
    }
}

impl IndexManager for Repo {
    fn stage(&self, paths: &[&str]) -> AppResult<()> {
        self.embedded.stage(paths)
    }

    fn unstage(&self, paths: &[&str]) -> AppResult<()> {
        self.embedded.unstage(paths)
    }

    fn staged_entries(&self) -> AppResult<Vec<StatusEntry>> {
        self.embedded.staged_entries()
    }
}

impl Committer for Repo {
    fn commit(&self, message: &str, opts: Option<&CommitOptions>) -> AppResult<Oid> {
        self.embedded.commit(message, opts)
    }
}

impl Merger for Repo {
    fn merge(&self, branch: &str, opts: Option<&MergeOptions>) -> AppResult<MergeResult> {
        self.cli.merge(branch, opts)
    }

    fn abort_merge(&self) -> AppResult<()> {
        self.cli.abort_merge()
    }
}

impl Rebaser for Repo {
    fn rebase(&self, onto: &str, opts: Option<&RebaseOptions>) -> AppResult<RebaseResult> {
        self.cli.rebase(onto, opts)
    }

    fn abort_rebase(&self) -> AppResult<()> {
        self.cli.abort_rebase()
    }

    fn continue_rebase(&self) -> AppResult<RebaseResult> {
        self.cli.continue_rebase()
    }
}

impl CherryPicker for Repo {
    fn cherry_pick(
        &self,
        commit: &str,
        opts: Option<&crate::options::CherryPickOptions>,
    ) -> AppResult<Oid> {
        self.cli.cherry_pick(commit, opts)
    }

    fn cherry_pick_continue(&self) -> AppResult<Oid> {
        self.cli.cherry_pick_continue()
    }

    fn cherry_pick_abort(&self) -> AppResult<()> {
        self.cli.cherry_pick_abort()
    }
}

impl Resetter for Repo {
    fn reset(&self, target: &str, mode: ResetMode) -> AppResult<()> {
        self.cli.reset(target, mode)
    }
}

impl CheckoutManager for Repo {
    fn checkout(&self, ref_name: &str, opts: Option<&CheckoutOptions>) -> AppResult<()> {
        self.cli.checkout(ref_name, opts)
    }

    fn checkout_files(&self, paths: &[&str]) -> AppResult<()> {
        self.cli.checkout_files(paths)
    }
}

impl Stasher for Repo {
    fn stash(&self, message: &str) -> AppResult<Oid> {
        self.cli.stash(message)
    }

    fn stash_pop(&self) -> AppResult<()> {
        self.cli.stash_pop()
    }

    fn stash_pop_index(&self, index: usize) -> AppResult<()> {
        self.cli.stash_pop_index(index)
    }

    fn stash_list(&self) -> AppResult<Vec<StashEntry>> {
        self.cli.stash_list()
    }
}

impl RefManager for Repo {
    fn list_branches(&self, filter: BranchFilter) -> AppResult<Vec<Branch>> {
        self.embedded.list_branches(filter)
    }

    fn list_tags(&self) -> AppResult<Vec<Tag>> {
        self.embedded.list_tags()
    }

    fn create_branch(&self, name: &str, target: &str) -> AppResult<()> {
        self.embedded.create_branch(name, target)
    }

    fn delete_branch(&self, name: &str) -> AppResult<()> {
        self.embedded.delete_branch(name)
    }

    fn create_tag(&self, name: &str, target: &str, message: Option<&str>) -> AppResult<()> {
        self.embedded.create_tag(name, target, message)
    }

    /// Creates a signed annotated tag by routing to the git CLI backend.
    ///
    /// Regular ref operations use the embedded backend, but signed tags require `git tag -s`, so `Repo` delegates this operation to `GitCli`.
    fn create_signed_tag(
        &self,
        name: &str,
        target: &str,
        message: &str,
        opts: &SignOptions,
    ) -> AppResult<()> {
        // Signing requires the git CLI (`git tag -s`); libgit2 cannot sign.
        self.cli.create_signed_tag(name, target, message, opts)
    }

    fn delete_tag(&self, name: &str) -> AppResult<()> {
        self.embedded.delete_tag(name)
    }
}

impl RemoteManager for Repo {
    fn list_remotes(&self) -> AppResult<Vec<Remote>> {
        self.embedded.list_remotes()
    }

    fn fetch(&self, remote: &str, opts: Option<&FetchOptions>) -> AppResult<()> {
        self.embedded.fetch(remote, opts)
    }

    fn push(&self, remote: &str, opts: Option<&PushOptions>) -> AppResult<()> {
        self.embedded.push(remote, opts)
    }

    fn tracking_branch(&self, branch: &str) -> AppResult<String> {
        self.embedded.tracking_branch(branch)
    }
}

impl ConfigReader for Repo {
    fn config_get(&self, key: &str) -> AppResult<String> {
        self.embedded.config_get(key)
    }

    fn config_get_all(&self, key: &str) -> AppResult<Vec<String>> {
        self.embedded.config_get_all(key)
    }

    fn config_set(&self, key: &str, value: &str) -> AppResult<()> {
        self.embedded.config_set(key, value)
    }
}

impl Maintainer for Repo {
    fn gc(&self) -> AppResult<()> {
        self.cli.gc()
    }

    fn prune(&self) -> AppResult<()> {
        self.cli.prune()
    }

    fn fsck(&self) -> AppResult<()> {
        self.cli.fsck()
    }

    fn clean(&self, opts: Option<&CleanOptions>) -> AppResult<Vec<String>> {
        self.cli.clean(opts)
    }
}
