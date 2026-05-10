//! Embedded `git2` backend.

pub mod auth;
mod manage;
mod read;
mod write;

use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use rskit_errors::AppResult;

use crate::core::Repository;
use crate::error::GitError;
use crate::types::{Commit, Oid, Reference, Signature};

/// Embedded repository backend backed by `git2`.
pub struct Backend {
    pub(crate) repo: git2::Repository,
    root: PathBuf,
}

impl Backend {
    /// Returns the repository root path.
    pub fn root(&self) -> &Path {
        &self.root
    }
}

/// Opens a git repository at the exact given path.
pub fn open(path: impl AsRef<Path>) -> AppResult<Backend> {
    let path = path.as_ref();
    let abs = std::fs::canonicalize(path).map_err(|_| GitError::NotFound {
        path: path.to_path_buf(),
    })?;
    let repo =
        git2::Repository::open(&abs).map_err(|_| GitError::NotFound { path: abs.clone() })?;
    // workdir() is None for bare repos; fall back to the .git dir path
    let root = repo.workdir().unwrap_or_else(|| repo.path()).to_path_buf();
    Ok(Backend { repo, root })
}

/// Discovers a git repository by walking up from the given path.
pub fn discover(path: impl AsRef<Path>) -> AppResult<Backend> {
    let path = path.as_ref();
    let repo = git2::Repository::discover(path).map_err(|_| GitError::NotFound {
        path: path.to_path_buf(),
    })?;
    // workdir() is None for bare repos; fall back to the .git dir path
    let root = repo.workdir().unwrap_or_else(|| repo.path()).to_path_buf();
    Ok(Backend { repo, root })
}

/// Clones a git repository into the given path.
pub fn clone(url: &str, path: impl AsRef<Path>) -> AppResult<Backend> {
    let path = path.as_ref();
    let repo = git2::Repository::clone(url, path).map_err(GitError::Internal)?;
    let root = repo
        .workdir()
        .map(Path::to_path_buf)
        .unwrap_or_else(|| path.to_path_buf());
    Ok(Backend { repo, root })
}

/// Creates a new git repository at the given path.
pub fn init(path: impl AsRef<Path>) -> AppResult<Backend> {
    let path = path.as_ref();
    let repo = git2::Repository::init(path).map_err(GitError::Internal)?;
    let root = repo
        .workdir()
        .map(Path::to_path_buf)
        .unwrap_or_else(|| path.to_path_buf());
    Ok(Backend { repo, root })
}

/// Creates a new bare git repository at the given path.
pub fn init_bare(path: impl AsRef<Path>) -> AppResult<Backend> {
    let path = path.as_ref();
    let repo = git2::Repository::init_bare(path).map_err(GitError::Internal)?;
    Ok(Backend {
        repo,
        root: path.to_path_buf(),
    })
}

impl Repository for Backend {
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
