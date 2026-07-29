//! Management trait implementations for the libgit2 repository.

use std::cell::RefCell;
use std::rc::Rc;

use git2::{BranchType, FetchPrune};
use rskit_errors::{AppError, AppResult};

use crate::error::GitError;
use crate::manage::{ConfigReader, RefManager, RemoteManager};
use crate::options::{FetchOptions, PushOptions};
use crate::types::{Branch, BranchFilter, Remote, Tag};

use super::{
    Git2Repository, map_push_error, map_remote_error, oid_from_git2, redact_url_credentials,
    signature_from_git2,
};

impl RefManager for Git2Repository {
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
                    .unwrap_or_else(|| oid_from_git2(git2::Oid::ZERO_SHA1));
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
        for name in collect_git_strings(names.iter())? {
            let reference = self
                .repo
                .find_reference(&format!("refs/tags/{}", name))
                .map_err(GitError::Internal)?;
            let target = reference.target().ok_or_else(|| GitError::RefNotFound {
                refname: name.clone(),
            })?;

            if let Ok(tag) = self.repo.find_tag(target) {
                tags.push(Tag {
                    name: name.clone(),
                    target: oid_from_git2(tag.target_id()),
                    tagger: tag.tagger().map(|sig| signature_from_git2(&sig)),
                    message: tag
                        .message()
                        .ok()
                        .flatten()
                        .unwrap_or_default()
                        .trim_end_matches('\n')
                        .to_string(),
                });
            } else {
                tags.push(Tag {
                    name,
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

    fn create_tag(&self, name: &str, target: &str, message: Option<&str>) -> AppResult<()> {
        let obj = self
            .repo
            .revparse_single(target)
            .map_err(|_| GitError::RefNotFound {
                refname: target.to_string(),
            })?;
        if let Some(message) = message {
            let signature = self.repo.signature().map_err(GitError::Internal)?;
            self.repo
                .tag(name, &obj, &signature, message, false)
                .map_err(|err| map_exists_error("tag", name, err))?;
        } else {
            self.repo
                .reference(
                    &format!("refs/tags/{name}"),
                    obj.id(),
                    false,
                    "create lightweight tag",
                )
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

impl RemoteManager for Git2Repository {
    fn list_remotes(&self) -> AppResult<Vec<Remote>> {
        let remotes = self.repo.remotes().map_err(GitError::Internal)?;
        let mut items = Vec::new();

        for name in collect_git_strings(remotes.iter())? {
            let remote = self.repo.find_remote(&name).map_err(GitError::Internal)?;
            items.push(Remote {
                name,
                url: remote.url().unwrap_or_default().to_string(),
                fetch_specs: collect_git_strings(
                    remote.fetch_refspecs().map_err(GitError::Internal)?.iter(),
                )?,
                push_specs: collect_git_strings(
                    remote.push_refspecs().map_err(GitError::Internal)?.iter(),
                )?,
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

        // libgit2 reports a server-side per-ref rejection ("ng <ref> <reason>")
        // through this callback while `remote.push` may still return `Ok`; a
        // rejection recorded here must therefore surface as a typed error rather
        // than a silent success.
        let rejections: Rc<RefCell<Vec<(String, String)>>> = Rc::new(RefCell::new(Vec::new()));
        let mut callbacks = git2::RemoteCallbacks::new();
        {
            let rejections = Rc::clone(&rejections);
            callbacks.push_update_reference(move |refname, status| {
                if let Some(reason) = status {
                    rejections
                        .borrow_mut()
                        .push((refname.to_string(), reason.to_string()));
                }
                Ok(())
            });
        }
        let mut push_opts = git2::PushOptions::new();
        push_opts.remote_callbacks(callbacks);

        let result = handle.push(&refspecs, Some(&mut push_opts));

        let rejections = rejections.borrow();
        if !rejections.is_empty() {
            // Name every rejected ref, and surface each distinct reason once —
            // a protected branch typically rejects every ref with one reason.
            // The seen-set keeps the first-seen order at O(n) rather than a
            // `Vec::contains` scan per reason.
            let mut seen = std::collections::HashSet::new();
            let refname = rejections
                .iter()
                .map(|(name, _)| name.clone())
                .collect::<Vec<_>>()
                .join(", ");
            let reason = rejections
                .iter()
                .map(|(_, reason)| redact_url_credentials(reason))
                .filter(|reason| seen.insert(reason.clone()))
                .collect::<Vec<_>>()
                .join("; ");
            return Err(GitError::PushRejected { refname, reason }.into());
        }
        drop(rejections);
        result.map_err(|err| map_push_error(err, &refspecs))?;
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

impl ConfigReader for Git2Repository {
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
            let value = entry.value().map_err(GitError::Internal)?;
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
        _ => collect_git_strings(remote.push_refspecs().map_err(GitError::Internal)?.iter())?,
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

fn collect_git_strings<'a>(
    iter: impl IntoIterator<Item = Result<Option<&'a str>, git2::Error>>,
) -> AppResult<Vec<String>> {
    let mut values = Vec::new();
    for item in iter {
        let Some(value) = item.map_err(GitError::Internal)? else {
            return Err(AppError::invalid_format("git string array", "utf-8 string"));
        };
        values.push(value.to_string());
    }
    Ok(values)
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
