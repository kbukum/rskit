//! Repository management traits.

use rskit_errors::AppResult;

use crate::options::{CleanOptions, FetchOptions, PushOptions};
use crate::types::{Branch, BranchFilter, Remote, Tag};

/// Read and manage git references.
pub trait RefManager {
    /// Lists branches matching the requested filter.
    fn list_branches(&self, filter: BranchFilter) -> AppResult<Vec<Branch>>;

    /// Lists tags in the repository.
    fn list_tags(&self) -> AppResult<Vec<Tag>>;

    /// Creates a local branch pointing at the given target revision.
    fn create_branch(&self, name: &str, target: &str) -> AppResult<()>;

    /// Deletes a local branch.
    fn delete_branch(&self, name: &str) -> AppResult<()>;

    /// Creates a tag pointing at the given target revision.
    /// `Some(message)` creates an annotated tag (with tagger and the given message, which may be empty);
    /// `None` creates a lightweight tag (a plain ref). Both backends must follow this convention.
    fn create_tag(&self, name: &str, target: &str, message: Option<&str>) -> AppResult<()>;

    /// Deletes a tag.
    fn delete_tag(&self, name: &str) -> AppResult<()>;
}

/// Read and manage git remotes.
pub trait RemoteManager {
    /// Lists configured remotes.
    fn list_remotes(&self) -> AppResult<Vec<Remote>>;

    /// Fetches updates from a remote.
    fn fetch(&self, remote: &str, opts: Option<&FetchOptions>) -> AppResult<()>;

    /// Pushes refs to a remote.
    fn push(&self, remote: &str, opts: Option<&PushOptions>) -> AppResult<()>;

    /// Returns the configured upstream tracking branch for a local branch.
    fn tracking_branch(&self, branch: &str) -> AppResult<String>;
}

/// Read and update git configuration.
pub trait ConfigReader {
    /// Returns the highest-precedence value for a config key.
    fn config_get(&self, key: &str) -> AppResult<String>;

    /// Returns all configured values for a multivar config key.
    fn config_get_all(&self, key: &str) -> AppResult<Vec<String>>;

    /// Sets a config key in the repository configuration.
    fn config_set(&self, key: &str, value: &str) -> AppResult<()>;
}

/// Repository maintenance operations.
pub trait Maintainer {
    /// Runs repository garbage collection.
    fn gc(&self) -> AppResult<()>;

    /// Prunes unreachable objects.
    fn prune(&self) -> AppResult<()>;

    /// Verifies repository object integrity.
    fn fsck(&self) -> AppResult<()>;

    /// Cleans untracked files according to the provided options.
    fn clean(&self, opts: Option<&CleanOptions>) -> AppResult<Vec<String>>;
}
