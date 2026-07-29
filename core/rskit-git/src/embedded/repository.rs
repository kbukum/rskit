use std::path::{Path, PathBuf};
use std::sync::Arc;

use rskit_errors::{AppError, AppResult};

use crate::auth::{AuthProvider, DefaultAuthProvider};
use crate::core::Repository as RepositoryTrait;
use crate::error::GitError;
use crate::options::InitOptions;
use crate::types::{DEFAULT_BRANCH, Oid, Reference};

use super::{map_head_error, oid_from_git2, reference_from_git2};

/// Repository implementation backed by libgit2.
pub struct Git2Repository {
    pub(crate) repo: git2::Repository,
    pub(crate) root: PathBuf,
    pub(crate) auth: Arc<dyn AuthProvider>,
}

impl Git2Repository {
    /// Returns the repository root path.
    pub fn root(&self) -> &Path {
        &self.root
    }
}

fn default_auth() -> Arc<dyn AuthProvider> {
    Arc::new(DefaultAuthProvider)
}

/// Opens a git repository at the given path (canonicalized).
pub fn open(path: impl AsRef<Path>) -> AppResult<Git2Repository> {
    open_with_auth(path, default_auth())
}

/// Opens a git repository at the given path with an explicit auth provider.
pub fn open_with_auth(
    path: impl AsRef<Path>,
    auth: Arc<dyn AuthProvider>,
) -> AppResult<Git2Repository> {
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
    Ok(Git2Repository { repo, root, auth })
}

/// Discovers a git repository by walking up from the given path.
pub fn discover(path: impl AsRef<Path>) -> AppResult<Git2Repository> {
    discover_with_auth(path, default_auth())
}

/// Discovers a git repository by walking up from `path` with an explicit auth provider.
pub fn discover_with_auth(
    path: impl AsRef<Path>,
    auth: Arc<dyn AuthProvider>,
) -> AppResult<Git2Repository> {
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
    Ok(Git2Repository { repo, root, auth })
}

/// Clones a git repository into the given path.
pub fn clone(url: &str, path: impl AsRef<Path>) -> AppResult<Git2Repository> {
    let path = path.as_ref();
    let repo = git2::Repository::clone(url, path).map_err(GitError::Internal)?;
    let root = repo
        .workdir()
        .map(Path::to_path_buf)
        .unwrap_or_else(|| path.to_path_buf());
    Ok(Git2Repository {
        repo,
        root,
        auth: default_auth(),
    })
}

/// Creates a new git repository at the given path.
///
/// The initial branch is [`DEFAULT_BRANCH`], pinned explicitly so the trunk
/// name does not depend on the host's Git configuration.
pub fn init(path: impl AsRef<Path>) -> AppResult<Git2Repository> {
    init_with(
        path,
        &InitOptions::default().with_initial_branch(DEFAULT_BRANCH),
    )
}

/// Creates a new git repository at the given path with explicit options.
pub fn init_with(path: impl AsRef<Path>, options: &InitOptions) -> AppResult<Git2Repository> {
    let path = path.as_ref();
    let mut git2_options = git2::RepositoryInitOptions::new();
    if let Some(initial_branch) = &options.initial_branch {
        let reference = format!("refs/heads/{initial_branch}");
        if !git2::Reference::is_valid_name(&reference) {
            return Err(AppError::invalid_input(
                "initial_branch",
                format!("invalid branch name '{initial_branch}'"),
            ));
        }
        git2_options.initial_head(initial_branch);
    }
    let repo = git2::Repository::init_opts(path, &git2_options).map_err(GitError::Internal)?;
    let root = repo
        .workdir()
        .map(Path::to_path_buf)
        .unwrap_or_else(|| path.to_path_buf());
    Ok(Git2Repository {
        repo,
        root,
        auth: default_auth(),
    })
}

/// Creates a new bare git repository at the given path.
///
/// The initial branch is [`DEFAULT_BRANCH`], matching [`init`].
pub fn init_bare(path: impl AsRef<Path>) -> AppResult<Git2Repository> {
    let path = path.as_ref();
    let mut options = git2::RepositoryInitOptions::new();
    options.bare(true).initial_head(DEFAULT_BRANCH);
    let repo = git2::Repository::init_opts(path, &options).map_err(GitError::Internal)?;
    Ok(Git2Repository {
        repo,
        root: path.to_path_buf(),
        auth: default_auth(),
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
