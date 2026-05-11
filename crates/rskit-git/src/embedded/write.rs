//! Write trait implementations for the embedded backend.

use std::path::Path;
use std::time::{Duration, SystemTime};

use rskit_errors::{AppError, AppResult};

use crate::error::GitError;
use crate::options::CommitOptions;
use crate::types::{EntryState, Oid, Signature, StatusEntry};
use crate::write::{Committer, IndexManager};

use super::{Backend, oid_from_git2};

impl IndexManager for Backend {
    fn stage(&self, paths: &[&str]) -> AppResult<()> {
        if self.repo.is_bare() {
            return Err(GitError::NotImplemented {
                operation: "stage on bare repository",
            }
            .into());
        }
        let mut index = self.repo.index().map_err(GitError::Internal)?;
        for path in paths {
            let p = Path::new(path);
            // Reject absolute paths and paths with parent (..) components
            if p.is_absolute() || p.components().any(|c| c == std::path::Component::ParentDir)
            {
                return Err(AppError::invalid_input(
                    "path",
                    format!("must be relative and inside the repository: {path}"),
                ));
            }
            if self.root.join(p).exists() {
                index.add_path(p).map_err(GitError::Internal)?;
            } else {
                index.remove_path(p).map_err(GitError::Internal)?;
            }
        }
        index.write().map_err(GitError::Internal)?;
        Ok(())
    }

    fn unstage(&self, paths: &[&str]) -> AppResult<()> {
        if paths.is_empty() {
            return Ok(());
        }

        let head = self.repo.revparse_single("HEAD").ok();
        self.repo
            .reset_default(head.as_ref(), paths.iter().copied())
            .map_err(GitError::Internal)?;
        Ok(())
    }

    fn staged_entries(&self) -> AppResult<Vec<StatusEntry>> {
        let mut opts = git2::StatusOptions::new();
        opts.include_untracked(false)
            .include_ignored(false)
            .recurse_untracked_dirs(false)
            .renames_head_to_index(true);

        let statuses = self
            .repo
            .statuses(Some(&mut opts))
            .map_err(GitError::Internal)?;
        let mut entries = Vec::new();

        for entry in statuses.iter() {
            let status = entry.status();
            let Some(path) = entry.path() else {
                continue;
            };

            if status.is_conflicted() {
                entries.push(StatusEntry {
                    path: path.to_string(),
                    state: EntryState::Conflicted,
                });
            } else if is_index_status(status) {
                entries.push(StatusEntry {
                    path: path.to_string(),
                    state: EntryState::Staged,
                });
            }
        }

        Ok(entries)
    }
}

impl Committer for Backend {
    fn commit(&self, message: &str, opts: Option<&CommitOptions>) -> AppResult<Oid> {
        let opts = opts.cloned().unwrap_or_default();
        if opts.sign {
            return Err(GitError::SigningNotSupported.into());
        }

        let mut index = self.repo.index().map_err(GitError::Internal)?;
        let tree_id = index.write_tree().map_err(GitError::Internal)?;
        let tree = self.repo.find_tree(tree_id).map_err(GitError::Internal)?;

        let oid = if opts.amend {
            let head = self.resolve_commit("HEAD")?;
            let author = opts.author.as_ref().map(signature_to_git2).transpose()?;
            let committer = match opts.committer.as_ref() {
                Some(signature) => Some(signature_to_git2(signature)?),
                None => Some(self.repo.signature().map_err(GitError::Internal)?),
            };

            head.amend(
                Some("HEAD"),
                author.as_ref(),
                committer.as_ref(),
                None,
                Some(message),
                Some(&tree),
            )
            .map_err(GitError::Internal)?
        } else {
            let author = match opts.author.as_ref() {
                Some(signature) => signature_to_git2(signature)?,
                None => self.repo.signature().map_err(GitError::Internal)?,
            };
            let committer = match opts.committer.as_ref() {
                Some(signature) => signature_to_git2(signature)?,
                None => self.repo.signature().map_err(GitError::Internal)?,
            };

            let parents = self
                .resolve_commit("HEAD")
                .ok()
                .into_iter()
                .collect::<Vec<_>>();
            let parent_refs = parents.iter().collect::<Vec<_>>();

            self.repo
                .commit(
                    Some("HEAD"),
                    &author,
                    &committer,
                    message,
                    &tree,
                    &parent_refs,
                )
                .map_err(GitError::Internal)?
        };

        Ok(oid_from_git2(oid))
    }
}

fn signature_to_git2(signature: &Signature) -> AppResult<git2::Signature<'static>> {
    let time = git2_time_from_system_time(signature.when);
    git2::Signature::new(&signature.name, &signature.email, &time)
        .map_err(GitError::Internal)
        .map_err(Into::into)
}

fn git2_time_from_system_time(time: SystemTime) -> git2::Time {
    match time.duration_since(SystemTime::UNIX_EPOCH) {
        Ok(duration) => git2::Time::new(duration.as_secs() as i64, 0),
        Err(error) => {
            let duration: Duration = error.duration();
            git2::Time::new(-(duration.as_secs() as i64), 0)
        }
    }
}

fn is_index_status(status: git2::Status) -> bool {
    status.intersects(
        git2::Status::INDEX_NEW
            | git2::Status::INDEX_MODIFIED
            | git2::Status::INDEX_DELETED
            | git2::Status::INDEX_RENAMED
            | git2::Status::INDEX_TYPECHANGE,
    )
}
