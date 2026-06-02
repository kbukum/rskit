//! Read trait implementations for the libgit2 repository.

use std::path::Path;
use std::time::SystemTime;

use rskit_errors::{AppError, AppResult};

use crate::error::GitError;
use crate::options::{BlameOptions, LogOptions};
use crate::read::{Blamer, Differ, IgnoreReader, IndexReader, LogReader, TreeReader};
use crate::types::{
    BlameLine, Commit, DiffEntry, DiffStats, EntryKind, EntryState, FileStatus, IndexEntry, Oid,
    StatusEntry, TreeEntry, TreeHash,
};

use super::{Git2Repository, commit_from_git2, oid_from_git2, signature_from_git2};

impl Differ for Git2Repository {
    fn diff(&self, from: &str, to: &str) -> AppResult<Vec<DiffEntry>> {
        let from_tree = self.tree_for_ref(from)?;
        let to_tree = self.tree_for_ref(to)?;
        let diff = self
            .repo
            .diff_tree_to_tree(Some(&from_tree), Some(&to_tree), None)
            .map_err(GitError::Internal)?;

        let mut entries = Vec::new();
        diff.foreach(
            &mut |delta, _| {
                entries.push(diff_entry_from_delta(&delta));
                true
            },
            None,
            None,
            None,
        )
        .map_err(GitError::Internal)?;

        Ok(entries)
    }

    fn diff_stats(&self, from: &str, to: &str) -> AppResult<DiffStats> {
        let from_tree = self.tree_for_ref(from)?;
        let to_tree = self.tree_for_ref(to)?;
        let diff = self
            .repo
            .diff_tree_to_tree(Some(&from_tree), Some(&to_tree), None)
            .map_err(GitError::Internal)?;
        let stats = diff.stats().map_err(GitError::Internal)?;

        Ok(DiffStats {
            additions: stats.insertions(),
            deletions: stats.deletions(),
            files_changed: stats.files_changed(),
        })
    }

    fn status(&self) -> AppResult<Vec<StatusEntry>> {
        let mut opts = git2::StatusOptions::new();
        opts.include_untracked(true)
            .include_ignored(false)
            .recurse_untracked_dirs(true)
            .renames_head_to_index(true)
            .renames_index_to_workdir(true);

        let statuses = self
            .repo
            .statuses(Some(&mut opts))
            .map_err(GitError::Internal)?;
        let mut entries = Vec::new();

        for entry in statuses.iter() {
            let path = entry.path().map_err(GitError::Internal)?;
            entries.push(StatusEntry {
                path: path.to_string(),
                state: entry_state_from_status(entry.status()),
            });
        }

        Ok(entries)
    }
}

impl IgnoreReader for Git2Repository {
    fn is_ignored(&self, path: &str) -> AppResult<bool> {
        self.repo
            .status_should_ignore(Path::new(path))
            .map_err(GitError::Internal)
            .map_err(Into::into)
    }
}

impl TreeReader for Git2Repository {
    fn tree_hash(&self, revision: &str, path: &str) -> AppResult<TreeHash> {
        let tree = self.resolve_tree(revision, path)?;
        Ok(oid_from_git2(tree.id()))
    }

    fn file_at(&self, revision: &str, path: &str) -> AppResult<Vec<u8>> {
        let obj = self
            .repo
            .revparse_single(revision)
            .map_err(|_| GitError::RefNotFound {
                refname: revision.to_string(),
            })?;
        let commit = obj.peel_to_commit().map_err(|_| GitError::RefNotFound {
            refname: revision.to_string(),
        })?;
        let tree = commit.tree().map_err(GitError::Internal)?;
        let entry = tree
            .get_path(Path::new(path))
            .map_err(|_| GitError::InvalidPath {
                path: path.to_string(),
            })?;
        let blob = self
            .repo
            .find_blob(entry.id())
            .map_err(GitError::Internal)?;
        Ok(blob.content().to_vec())
    }

    fn list_entries(&self, revision: &str, path: &str) -> AppResult<Vec<TreeEntry>> {
        let tree = self.resolve_tree(revision, path)?;
        Ok(tree
            .iter()
            .map(|entry| TreeEntry {
                name: entry.name().unwrap_or("").to_string(),
                oid: oid_from_git2(entry.id()),
                kind: entry_kind_from_filemode(entry.filemode()),
                filemode: entry.filemode() as u32,
            })
            .collect())
    }
}

impl IndexReader for Git2Repository {
    fn index_entry(&self, path: &str) -> AppResult<Option<IndexEntry>> {
        let index = self.repo.index().map_err(GitError::Internal)?;
        Ok(index.get_path(Path::new(path), 0).map(|entry| IndexEntry {
            path: path.to_string(),
            oid: oid_from_git2(entry.id),
            kind: entry_kind_from_filemode(entry.mode as i32),
            filemode: entry.mode,
        }))
    }
}

impl LogReader for Git2Repository {
    fn log(&self, opts: Option<&LogOptions>) -> AppResult<Vec<Commit>> {
        let opts = opts.cloned().unwrap_or_default();
        let mut walk = self.repo.revwalk().map_err(GitError::Internal)?;
        walk.set_sorting(git2::Sort::TIME | git2::Sort::TOPOLOGICAL)
            .map_err(GitError::Internal)?;
        walk.push_head().map_err(GitError::Internal)?;

        let mut commits = Vec::new();
        for oid in walk {
            let oid = oid.map_err(GitError::Internal)?;
            let commit = self.repo.find_commit(oid).map_err(GitError::Internal)?;
            if self.commit_matches_log_options(&commit, &opts)? {
                commits.push(commit_from_git2(&commit));
                if opts.max_count.is_some_and(|max| commits.len() >= max) {
                    break;
                }
            }
        }

        Ok(commits)
    }

    fn merge_base(&self, a: &str, b: &str) -> AppResult<Oid> {
        let a_rev = a.to_string();
        let b_rev = b.to_string();
        let a = self.resolve_commit(a)?;
        let b = self.resolve_commit(b)?;
        self.repo
            .merge_base(a.id(), b.id())
            .map(oid_from_git2)
            .map_err(|e| {
                if e.code() == git2::ErrorCode::NotFound {
                    GitError::NoMergeBase { a: a_rev, b: b_rev }
                } else {
                    GitError::Internal(e)
                }
            })
            .map_err(Into::into)
    }

    fn is_ancestor(&self, ancestor: &str, descendant: &str) -> AppResult<bool> {
        let ancestor = self.resolve_commit(ancestor)?;
        let descendant = self.resolve_commit(descendant)?;
        self.repo
            .graph_descendant_of(descendant.id(), ancestor.id())
            .map_err(GitError::Internal)
            .map_err(Into::into)
    }
}

impl Blamer for Git2Repository {
    fn blame(
        &self,
        revision: &str,
        path: &str,
        opts: Option<&BlameOptions>,
    ) -> AppResult<Vec<BlameLine>> {
        let commit = self.resolve_commit(revision)?;
        let file = String::from_utf8_lossy(&self.file_at(revision, path)?).into_owned();
        let lines: Vec<&str> = file.lines().collect();

        let mut blame_opts = git2::BlameOptions::new();
        blame_opts.newest_commit(commit.id());
        if let Some(opts) = opts {
            if let (Some(start), Some(end)) = (opts.start_line, opts.end_line)
                && start > end
            {
                return Err(GitError::InvalidLineRange { start, end }.into());
            }
            if let Some(start_line) = opts.start_line {
                blame_opts.min_line(start_line);
            }
            if let Some(end_line) = opts.end_line {
                blame_opts.max_line(end_line);
            }
            blame_opts.ignore_whitespace(opts.ignore_whitespace);
        }

        let blame = self
            .repo
            .blame_file(Path::new(path), Some(&mut blame_opts))
            .map_err(GitError::Internal)?;

        let mut entries = Vec::new();
        for hunk in blame.iter() {
            let signature = hunk
                .final_signature()
                .as_ref()
                .map(signature_from_git2)
                .ok_or_else(|| AppError::invalid_format("blame signature", "author signature"))?;
            let commit_oid = oid_from_git2(hunk.final_commit_id());
            let start = hunk.final_start_line();
            let end = start + hunk.lines_in_hunk();
            for line in start..end {
                let content = lines.get(line - 1).copied().unwrap_or_default().to_string();
                entries.push(BlameLine {
                    line,
                    commit_oid,
                    author: signature.clone(),
                    content,
                });
            }
        }

        Ok(entries)
    }
}

impl Git2Repository {
    pub(crate) fn tree_for_ref(&self, refname: &str) -> AppResult<git2::Tree<'_>> {
        let obj = self
            .repo
            .revparse_single(refname)
            .map_err(|_| GitError::RefNotFound {
                refname: refname.to_string(),
            })?;
        if let Ok(tree) = obj.peel_to_tree() {
            return Ok(tree);
        }
        let commit = obj.peel_to_commit().map_err(|_| GitError::RefNotFound {
            refname: refname.to_string(),
        })?;
        commit.tree().map_err(|e| GitError::Internal(e).into())
    }

    pub(crate) fn resolve_commit(&self, revision: &str) -> AppResult<git2::Commit<'_>> {
        let obj = self
            .repo
            .revparse_single(revision)
            .map_err(|_| GitError::RefNotFound {
                refname: revision.to_string(),
            })?;
        obj.peel_to_commit()
            .map_err(|_| GitError::RefNotFound {
                refname: revision.to_string(),
            })
            .map_err(Into::into)
    }

    fn resolve_tree<'repo>(
        &'repo self,
        revision: &str,
        path: &str,
    ) -> AppResult<git2::Tree<'repo>> {
        let obj = self
            .repo
            .revparse_single(revision)
            .map_err(|_| GitError::RefNotFound {
                refname: revision.to_string(),
            })?;
        let tree = if let Ok(tree) = obj.peel_to_tree() {
            tree
        } else {
            let commit = obj.peel_to_commit().map_err(|_| GitError::RefNotFound {
                refname: revision.to_string(),
            })?;
            commit.tree().map_err(GitError::Internal)?
        };

        if path.is_empty() || path == "." || path == "/" {
            return Ok(tree);
        }

        let entry = tree
            .get_path(Path::new(path))
            .map_err(|_| GitError::InvalidPath {
                path: path.to_string(),
            })?;
        self.repo
            .find_tree(entry.id())
            .map_err(|_| GitError::InvalidPath {
                path: path.to_string(),
            })
            .map_err(Into::into)
    }

    fn commit_matches_log_options(
        &self,
        commit: &git2::Commit<'_>,
        opts: &LogOptions,
    ) -> AppResult<bool> {
        if let Some(author_filter) = &opts.author_filter {
            let author_filter = author_filter.to_lowercase();
            let author = commit.author();
            let name = author.name().unwrap_or_default().to_lowercase();
            let email = author.email().unwrap_or_default().to_lowercase();
            if !name.contains(&author_filter) && !email.contains(&author_filter) {
                return Ok(false);
            }
        }

        let commit_time = commit.time();
        if let Some(since) = opts.since
            && git2_time_to_system_time(commit_time) < since
        {
            return Ok(false);
        }
        if let Some(until) = opts.until
            && git2_time_to_system_time(commit_time) > until
        {
            return Ok(false);
        }

        if let Some(path_filter) = &opts.path_filter {
            return self.commit_touches_path(commit, Path::new(path_filter));
        }

        Ok(true)
    }

    fn commit_touches_path(&self, commit: &git2::Commit<'_>, path: &Path) -> AppResult<bool> {
        let commit_tree = commit.tree().map_err(GitError::Internal)?;

        if commit.parent_count() == 0 {
            let mut diff_opts = git2::DiffOptions::new();
            diff_opts.pathspec(path);
            let diff = self
                .repo
                .diff_tree_to_tree(None, Some(&commit_tree), Some(&mut diff_opts))
                .map_err(GitError::Internal)?;
            return Ok(diff.deltas().len() > 0);
        }

        for parent in commit.parents() {
            let parent_tree = parent.tree().map_err(GitError::Internal)?;
            let mut diff_opts = git2::DiffOptions::new();
            diff_opts.pathspec(path);
            let diff = self
                .repo
                .diff_tree_to_tree(Some(&parent_tree), Some(&commit_tree), Some(&mut diff_opts))
                .map_err(GitError::Internal)?;
            if diff.deltas().len() > 0 {
                return Ok(true);
            }
        }

        Ok(false)
    }
}

fn diff_entry_from_delta(delta: &git2::DiffDelta<'_>) -> DiffEntry {
    let status = match delta.status() {
        git2::Delta::Added => FileStatus::Added,
        git2::Delta::Deleted => FileStatus::Deleted,
        git2::Delta::Modified => FileStatus::Modified,
        git2::Delta::Renamed => FileStatus::Renamed,
        git2::Delta::Copied => FileStatus::Copied,
        git2::Delta::Untracked => FileStatus::Untracked,
        git2::Delta::Ignored => FileStatus::Ignored,
        git2::Delta::Typechange => FileStatus::TypeChanged,
        git2::Delta::Conflicted => FileStatus::Conflicted,
        _ => FileStatus::Modified,
    };

    let path = delta
        .new_file()
        .path()
        .or_else(|| delta.old_file().path())
        .map(|path| path.to_string_lossy().into_owned())
        .unwrap_or_default();

    let old_path = if matches!(status, FileStatus::Renamed | FileStatus::Copied) {
        delta
            .old_file()
            .path()
            .map(|path| path.to_string_lossy().into_owned())
    } else {
        None
    };

    DiffEntry {
        path,
        old_path,
        old_oid: oid_from_git2(delta.old_file().id()),
        new_oid: oid_from_git2(delta.new_file().id()),
        status,
    }
}

fn entry_state_from_status(status: git2::Status) -> EntryState {
    if status.is_conflicted() {
        EntryState::Conflicted
    } else if status.intersects(
        git2::Status::INDEX_NEW
            | git2::Status::INDEX_MODIFIED
            | git2::Status::INDEX_DELETED
            | git2::Status::INDEX_RENAMED
            | git2::Status::INDEX_TYPECHANGE,
    ) {
        EntryState::Staged
    } else if status.is_wt_new() {
        EntryState::Untracked
    } else {
        EntryState::Unstaged
    }
}

fn entry_kind_from_filemode(mode: i32) -> EntryKind {
    match mode {
        0o040000 => EntryKind::Tree,
        0o160000 => EntryKind::Submodule,
        _ => EntryKind::Blob,
    }
}

fn git2_time_to_system_time(time: git2::Time) -> SystemTime {
    let seconds = time.seconds();
    if seconds >= 0 {
        SystemTime::UNIX_EPOCH + std::time::Duration::from_secs(seconds as u64)
    } else {
        SystemTime::UNIX_EPOCH - std::time::Duration::from_secs(seconds.unsigned_abs())
    }
}

#[cfg(test)]
mod tests {
    use std::fs;

    use crate::read::{Differ, IgnoreReader};

    #[test]
    fn status_includes_untracked_files_but_excludes_ignored_paths() {
        let root = rskit_fs::TempDir::new().expect("temp dir");
        let repo = super::super::init(root.path()).expect("init repo");
        fs::write(root.path().join(".gitignore"), "target/\n").expect("write ignore");
        fs::write(root.path().join("visible.rs"), "fn main() {}\n").expect("write source");
        fs::create_dir_all(root.path().join("target/debug")).expect("create target");
        fs::write(root.path().join("target/debug/app"), "binary\n").expect("write ignored file");

        let paths = repo
            .status()
            .expect("read status")
            .into_iter()
            .map(|entry| entry.path)
            .collect::<Vec<_>>();

        assert!(paths.iter().any(|path| path == "visible.rs"));
        assert!(!paths.iter().any(|path| path.starts_with("target/")));
    }

    #[test]
    fn ignore_reader_reports_gitignored_paths_without_requiring_files() {
        let root = rskit_fs::TempDir::new().expect("temp dir");
        let repo = super::super::init(root.path()).expect("init repo");
        fs::write(root.path().join(".gitignore"), "target/\n").expect("write ignore");
        fs::create_dir_all(root.path().join("target/debug")).expect("create target");
        fs::write(root.path().join("visible.rs"), "fn main() {}\n").expect("write source");

        assert!(
            repo.is_ignored("target/debug/app")
                .expect("check ignored nested path")
        );
        assert!(
            repo.is_ignored("target/missing.txt")
                .expect("check ignored missing path")
        );
        assert!(!repo.is_ignored("visible.rs").expect("check visible path"));
    }
}
