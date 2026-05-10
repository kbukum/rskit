//! Management trait implementations for the embedded backend.

use git2::{BranchType, FetchPrune};
use rskit_errors::{AppError, AppResult};

use crate::error::GitError;
use crate::manage::{ConfigReader, RefManager, RemoteManager};
use crate::options::{FetchOptions, PushOptions};
use crate::types::{Branch, BranchFilter, Remote, Tag};

use super::{Backend, map_remote_error, oid_from_git2, signature_from_git2};

impl RefManager for Backend {
    fn list_branches(&self, filter: BranchFilter) -> AppResult<Vec<Branch>> {
        let branch_filter = match filter {
            BranchFilter::Local => Some(BranchType::Local),
            BranchFilter::Remote => Some(BranchType::Remote),
            BranchFilter::All => None,
        };

        let mut branches = self
            .repo
            .branches(branch_filter)
            .map_err(GitError::Internal)?
            .map(|item| {
                let (branch, kind) = item.map_err(GitError::Internal)?;
                let name = branch
                    .name()
                    .map_err(GitError::Internal)?
                    .unwrap_or_default()
                    .to_string();
                let target = branch
                    .get()
                    .target()
                    .map(oid_from_git2)
                    .unwrap_or_else(|| oid_from_git2(git2::Oid::zero()));
                let upstream = if kind == BranchType::Local {
                    branch
                        .upstream()
                        .ok()
                        .and_then(|upstream| upstream.name().ok().flatten().map(str::to_string))
                } else {
                    None
                };
                Ok(Branch {
                    name,
                    target,
                    upstream,
                })
            })
            .collect::<Result<Vec<_>, GitError>>()
            .map_err::<rskit_errors::AppError, _>(Into::into)?;
        branches.sort_by(|a, b| a.name.cmp(&b.name));
        Ok(branches)
    }

    fn list_tags(&self) -> AppResult<Vec<Tag>> {
        let mut tags = Vec::new();
        let names = self.repo.tag_names(None).map_err(GitError::Internal)?;
        for name in names.iter().flatten() {
            let reference = self
                .repo
                .find_reference(&format!("refs/tags/{name}"))
                .map_err(GitError::Internal)?;
            let target = reference.target().ok_or_else(|| GitError::RefNotFound {
                refname: name.to_string(),
            })?;

            if let Ok(tag) = self.repo.find_tag(target) {
                tags.push(Tag {
                    name: name.to_string(),
                    target: oid_from_git2(tag.target_id()),
                    tagger: tag.tagger().map(|sig| signature_from_git2(&sig)),
                    message: tag
                        .message()
                        .unwrap_or_default()
                        .trim_end_matches('\n')
                        .to_string(),
                });
            } else {
                tags.push(Tag {
                    name: name.to_string(),
                    target: oid_from_git2(target),
                    tagger: None,
                    message: String::new(),
                });
            }
        }
        tags.sort_by(|a, b| a.name.cmp(&b.name));
        Ok(tags)
    }

    fn create_branch(&self, name: &str, target: &str) -> AppResult<()> {
        let commit = self.resolve_commit(target)?;
        self.repo
            .branch(name, &commit, false)
            .map_err(|err| map_exists_error("branch", name, err))?;
        Ok(())
    }

    fn delete_branch(&self, name: &str) -> AppResult<()> {
        let mut branch = self
            .repo
            .find_branch(name, BranchType::Local)
            .map_err(|_| GitError::RefNotFound {
                refname: name.to_string(),
            })?;
        branch
            .delete()
            .map_err(|err| map_branch_delete_error(name, err))?;
        Ok(())
    }

    fn create_tag(&self, name: &str, target: &str, message: &str) -> AppResult<()> {
        let obj = self
            .repo
            .revparse_single(target)
            .map_err(|_| GitError::RefNotFound {
                refname: target.to_string(),
            })?;
        if message.is_empty() {
            self.repo
                .reference(
                    &format!("refs/tags/{name}"),
                    obj.id(),
                    false,
                    "create lightweight tag",
                )
                .map_err(|err| map_exists_error("tag", name, err))?;
        } else {
            let signature = self.repo.signature().map_err(GitError::Internal)?;
            self.repo
                .tag(name, &obj, &signature, message, false)
                .map_err(|err| map_exists_error("tag", name, err))?;
        }
        Ok(())
    }

    fn delete_tag(&self, name: &str) -> AppResult<()> {
        self.repo.tag_delete(name).map_err(|e| {
            if e.code() == git2::ErrorCode::NotFound {
                GitError::RefNotFound {
                    refname: name.to_string(),
                }
            } else {
                GitError::Internal(e)
            }
        })?;
        Ok(())
    }
}

impl RemoteManager for Backend {
    fn list_remotes(&self) -> AppResult<Vec<Remote>> {
        let remotes = self.repo.remotes().map_err(GitError::Internal)?;
        let mut items = Vec::new();

        for name in remotes.iter().flatten() {
            let remote = self.repo.find_remote(name).map_err(GitError::Internal)?;
            items.push(Remote {
                name: name.to_string(),
                url: remote.url().unwrap_or_default().to_string(),
                fetch_specs: remote
                    .fetch_refspecs()
                    .map_err(GitError::Internal)?
                    .iter()
                    .flatten()
                    .map(str::to_string)
                    .collect(),
                push_specs: remote
                    .push_refspecs()
                    .map_err(GitError::Internal)?
                    .iter()
                    .flatten()
                    .map(str::to_string)
                    .collect(),
            });
        }

        items.sort_by(|a, b| a.name.cmp(&b.name));
        Ok(items)
    }

    fn fetch(&self, remote: &str, opts: Option<&FetchOptions>) -> AppResult<()> {
        let mut handle = self
            .repo
            .find_remote(remote)
            .map_err(|_| GitError::RemoteNotFound {
                name: remote.to_string(),
            })?;
        let refspecs = opts
            .map(|opts| opts.refspecs.iter().map(String::as_str).collect::<Vec<_>>())
            .unwrap_or_default();

        let mut fetch_opts = opts.map(fetch_options_to_git2).transpose()?;
        handle
            .fetch(&refspecs, fetch_opts.as_mut(), None)
            .map_err(map_remote_error)?;
        Ok(())
    }

    fn push(&self, remote: &str, opts: Option<&PushOptions>) -> AppResult<()> {
        let mut handle = self
            .repo
            .find_remote(remote)
            .map_err(|_| GitError::RemoteNotFound {
                name: remote.to_string(),
            })?;
        let refspecs = push_refspecs(&handle, opts)?;
        let mut push_opts = git2::PushOptions::new();
        handle
            .push(&refspecs, Some(&mut push_opts))
            .map_err(map_remote_error)?;
        Ok(())
    }

    fn tracking_branch(&self, branch: &str) -> AppResult<String> {
        let branch = self
            .repo
            .find_branch(branch, BranchType::Local)
            .map_err(|_| GitError::RefNotFound {
                refname: branch.to_string(),
            })?;
        let upstream = branch.upstream().map_err(|_| GitError::RefNotFound {
            refname: format!(
                "{branch}@{{upstream}}",
                branch = branch.name().ok().flatten().unwrap_or_default()
            ),
        })?;
        upstream
            .name()
            .map_err(GitError::Internal)?
            .map(str::to_string)
            .ok_or_else(|| {
                AppError::invalid_input("branch", "upstream branch name is not valid utf-8")
            })
    }
}

impl ConfigReader for Backend {
    fn config_get(&self, key: &str) -> AppResult<String> {
        let config = self.repo.config().map_err(GitError::Internal)?;
        config
            .get_string(key)
            .map_err(|err| map_config_error(key, err))
    }

    fn config_get_all(&self, key: &str) -> AppResult<Vec<String>> {
        let config = self.repo.config().map_err(GitError::Internal)?;
        let mut entries = config
            .multivar(key, None)
            .map_err(|err| map_config_error(key, err))?;
        let mut values = Vec::new();

        while let Some(entry) = entries.next() {
            let entry = entry.map_err(GitError::Internal)?;
            let value = entry
                .value()
                .ok_or_else(|| AppError::invalid_input(key, "config value is not valid utf-8"))?;
            values.push(value.to_string());
        }

        Ok(values)
    }

    fn config_set(&self, key: &str, value: &str) -> AppResult<()> {
        let mut config = self.repo.config().map_err(GitError::Internal)?;
        config
            .set_str(key, value)
            .map_err(|err| map_config_error(key, err))?;
        Ok(())
    }
}

fn fetch_options_to_git2(opts: &FetchOptions) -> AppResult<git2::FetchOptions<'static>> {
    let mut fetch_opts = git2::FetchOptions::new();
    fetch_opts.prune(if opts.prune {
        FetchPrune::On
    } else {
        FetchPrune::Off
    });

    if let Some(depth) = opts.depth {
        let depth = i32::try_from(depth)
            .map_err(|_| AppError::invalid_input("depth", "fetch depth exceeds supported range"))?;
        fetch_opts.depth(depth);
    }

    Ok(fetch_opts)
}

fn push_refspecs(remote: &git2::Remote<'_>, opts: Option<&PushOptions>) -> AppResult<Vec<String>> {
    let mut refspecs = match opts {
        Some(o) if !o.refspecs.is_empty() => o.refspecs.clone(),
        _ => remote
            .push_refspecs()
            .map_err(GitError::Internal)?
            .iter()
            .flatten()
            .map(str::to_string)
            .collect::<Vec<_>>(),
    };

    let force = opts.is_some_and(|o| o.force);
    if force {
        for refspec in &mut refspecs {
            if !refspec.starts_with('+') {
                refspec.insert(0, '+');
            }
        }
    }

    Ok(refspecs)
}

fn map_config_error(key: &str, err: git2::Error) -> rskit_errors::AppError {
    if err.code() == git2::ErrorCode::NotFound {
        GitError::ConfigNotFound {
            key: key.to_string(),
        }
        .into()
    } else {
        GitError::Internal(err).into()
    }
}

fn map_exists_error(kind: &'static str, name: &str, err: git2::Error) -> GitError {
    if err.code() == git2::ErrorCode::Exists {
        GitError::AlreadyExists {
            kind,
            name: name.to_string(),
        }
    } else {
        GitError::Internal(err)
    }
}

fn map_branch_delete_error(name: &str, err: git2::Error) -> GitError {
    if err.code() == git2::ErrorCode::NotFound {
        GitError::RefNotFound {
            refname: name.to_string(),
        }
    } else if err.message().contains("is checked out") {
        GitError::CheckedOutBranch {
            name: name.to_string(),
        }
    } else {
        GitError::Internal(err)
    }
}
