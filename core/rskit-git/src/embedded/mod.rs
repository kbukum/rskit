//! Embedded libgit2 repository implementation.

pub mod auth;
mod manage;
mod read;
mod write;

use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use rskit_errors::AppResult;

use crate::core::Repository as RepositoryTrait;
use crate::error::GitError;
use crate::types::{Commit, Oid, Reference, Signature};

/// Repository implementation backed by libgit2.
pub struct Git2Repository {
    pub(crate) repo: git2::Repository,
    root: PathBuf,
}

impl Git2Repository {
    /// Returns the repository root path.
    pub fn root(&self) -> &Path {
        &self.root
    }
}

/// Opens a git repository at the given path (canonicalized).
pub fn open(path: impl AsRef<Path>) -> AppResult<Git2Repository> {
    let path = path.as_ref();
    let abs = std::fs::canonicalize(path).map_err(|err| match err.kind() {
        std::io::ErrorKind::NotFound => GitError::NotFound {
            path: path.to_path_buf(),
        },
        _ => GitError::Internal(git2::Error::from_str(&err.to_string())),
    })?;
    let repo = git2::Repository::open(&abs).map_err(|err| {
        if err.code() == git2::ErrorCode::NotFound {
            GitError::NotFound { path: abs.clone() }
        } else {
            GitError::Internal(err)
        }
    })?;
    // workdir() is None for bare repos; fall back to the .git dir path
    let root = repo.workdir().unwrap_or_else(|| repo.path()).to_path_buf();
    Ok(Git2Repository { repo, root })
}

/// Discovers a git repository by walking up from the given path.
pub fn discover(path: impl AsRef<Path>) -> AppResult<Git2Repository> {
    let path = path.as_ref();
    let repo = git2::Repository::discover(path).map_err(|err| {
        if err.code() == git2::ErrorCode::NotFound {
            GitError::NotFound {
                path: path.to_path_buf(),
            }
        } else {
            GitError::Internal(err)
        }
    })?;
    // workdir() is None for bare repos; fall back to the .git dir path
    let root = repo.workdir().unwrap_or_else(|| repo.path()).to_path_buf();
    Ok(Git2Repository { repo, root })
}

/// Clones a git repository into the given path.
pub fn clone(url: &str, path: impl AsRef<Path>) -> AppResult<Git2Repository> {
    let path = path.as_ref();
    let repo = git2::Repository::clone(url, path).map_err(GitError::Internal)?;
    let root = repo
        .workdir()
        .map(Path::to_path_buf)
        .unwrap_or_else(|| path.to_path_buf());
    Ok(Git2Repository { repo, root })
}

/// Creates a new git repository at the given path.
pub fn init(path: impl AsRef<Path>) -> AppResult<Git2Repository> {
    let path = path.as_ref();
    let repo = git2::Repository::init(path).map_err(GitError::Internal)?;
    let root = repo
        .workdir()
        .map(Path::to_path_buf)
        .unwrap_or_else(|| path.to_path_buf());
    Ok(Git2Repository { repo, root })
}

/// Creates a new bare git repository at the given path.
pub fn init_bare(path: impl AsRef<Path>) -> AppResult<Git2Repository> {
    let path = path.as_ref();
    let repo = git2::Repository::init_bare(path).map_err(GitError::Internal)?;
    Ok(Git2Repository {
        repo,
        root: path.to_path_buf(),
    })
}

impl RepositoryTrait for Git2Repository {
    fn root(&self) -> &Path {
        &self.root
    }

    fn head(&self) -> AppResult<Reference> {
        let head = self.repo.head().map_err(map_head_error)?;
        Ok(reference_from_git2(&head))
    }

    fn resolve_ref(&self, refname: &str) -> AppResult<Oid> {
        let obj = self
            .repo
            .revparse_single(refname)
            .map_err(|_| GitError::RefNotFound {
                refname: refname.to_string(),
            })?;
        Ok(oid_from_git2(obj.id()))
    }

    fn is_dirty(&self) -> AppResult<bool> {
        let statuses = self.repo.statuses(None).map_err(GitError::Internal)?;
        Ok(!statuses.is_empty())
    }
}

pub(crate) fn oid_from_git2(oid: git2::Oid) -> Oid {
    let mut bytes = [0u8; 20];
    bytes.copy_from_slice(oid.as_bytes());
    Oid::from_bytes(bytes)
}

pub(crate) fn signature_from_git2(signature: &git2::Signature<'_>) -> Signature {
    Signature {
        name: signature.name().unwrap_or_default().to_string(),
        email: signature.email().unwrap_or_default().to_string(),
        when: system_time_from_git2(signature.when()),
    }
}

pub(crate) fn commit_from_git2(commit: &git2::Commit<'_>) -> Commit {
    Commit {
        oid: oid_from_git2(commit.id()),
        author: signature_from_git2(&commit.author()),
        committer: signature_from_git2(&commit.committer()),
        message: commit
            .message()
            .unwrap_or_default()
            .trim_end_matches('\n')
            .trim_end_matches('\0')
            .to_string(),
        parents: commit.parent_ids().map(oid_from_git2).collect(),
    }
}

fn reference_from_git2(reference: &git2::Reference<'_>) -> Reference {
    let name = reference.name().unwrap_or("HEAD").to_string();
    let target = reference
        .resolve()
        .ok()
        .and_then(|resolved| resolved.target())
        .or_else(|| reference.target())
        .map(oid_from_git2)
        .unwrap_or_else(|| Oid::from_bytes([0; 20]));
    Reference {
        name,
        target,
        is_branch: reference.is_branch(),
        is_tag: reference.is_tag(),
    }
}

fn map_head_error(err: git2::Error) -> GitError {
    if err.code() == git2::ErrorCode::UnbornBranch || err.code() == git2::ErrorCode::NotFound {
        GitError::RefNotFound {
            refname: "HEAD".to_string(),
        }
    } else {
        GitError::Internal(err)
    }
}

pub(crate) fn map_remote_error(err: git2::Error) -> GitError {
    if err.class() == git2::ErrorClass::Net {
        GitError::Network(err.message().to_string())
    } else {
        GitError::Internal(err)
    }
}

fn system_time_from_git2(time: git2::Time) -> SystemTime {
    let seconds = time.seconds();
    if seconds >= 0 {
        UNIX_EPOCH + Duration::from_secs(seconds as u64)
    } else {
        UNIX_EPOCH - Duration::from_secs(seconds.unsigned_abs())
    }
}

#[cfg(test)]
mod tests {
    use crate::core::Repository;

    use super::*;

    #[test]
    fn head_on_unborn_repository_maps_to_ref_not_found() {
        let root = rskit_testutil::test_workspace!("git2-unborn-head");
        let repo = init(root.path()).expect("init repo");

        let err = repo.head().expect_err("HEAD is unborn");

        assert_eq!(err.code(), rskit_errors::ErrorCode::NotFound);
    }

    #[test]
    fn system_time_from_git2_handles_negative_offsets() {
        let before_epoch = system_time_from_git2(git2::Time::new(-5, 0));

        assert_eq!(before_epoch, UNIX_EPOCH - Duration::from_secs(5));
    }

    #[test]
    fn map_remote_error_distinguishes_network_from_internal_errors() {
        let net = git2::Error::new(git2::ErrorCode::GenericError, git2::ErrorClass::Net, "down");
        assert!(matches!(map_remote_error(net), GitError::Network(_)));

        let other = git2::Error::from_str("other");
        assert!(matches!(map_remote_error(other), GitError::Internal(_)));
    }
}
