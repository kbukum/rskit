//! Composable git repository interfaces backed by libgit2 and the git CLI.
//!
//! This crate provides capability-based traits for git operations:
//! - [`Repository`] and [`Executor`] for core repository access and raw CLI execution
//! - [`Differ`], [`IgnoreReader`], [`TreeReader`], [`LogReader`], [`Blamer`], and [`Inspector`] for read flows
//! - [`IndexManager`], [`Committer`], and related write traits for mutating operations
//! - [`RefManager`], [`RemoteManager`], [`ConfigReader`], and [`Maintainer`] for management
//!
//! The default backend uses `git2` (libgit2) for embedded operations and delegates several
//! core mutating operations (merge, rebase, checkout, reset, stash, cherry-pick, maintenance)
//! to the `git` CLI binary. **A `git` binary must be available on `PATH` at runtime.**
//! Network operations (fetch, push) use the embedded `git2` backend.
//!
//! # Usage
//!
//! ```no_run
//! use rskit_git::{Differ, Repository, open};
//!
//! let repo = open("/path/to/repo").unwrap();
//! let dirty = repo.is_dirty().unwrap();
//! let _status = repo.status().unwrap();
//! assert!(!repo.root().as_os_str().is_empty() || !dirty);
//! ```

#![warn(missing_docs)]

pub mod auth;
pub mod cli;
pub mod core;
pub mod embedded;
pub mod error;
pub mod manage;
pub mod options;
pub mod paths;
pub mod read;
pub mod repo;
#[cfg(feature = "testutil")]
pub mod testutil;
pub mod types;
pub mod write;

pub use core::{Executor, Repository};
pub use error::GitError;
pub use manage::{ConfigReader, Maintainer, RefManager, RemoteManager};
pub use options::*;
pub use paths::{join_repo_path, repo_relative_path};
pub use read::{Blamer, Differ, IgnoreReader, IndexReader, Inspector, LogReader, TreeReader};
pub use repo::{Repo, clone, discover, init, init_bare, open};
pub use rskit_errors::{AppError, AppResult};
pub use types::*;
pub use write::{
    CheckoutManager, CherryPicker, Committer, IndexManager, Merger, Rebaser, Resetter, Stasher,
};

#[cfg(test)]
mod tests {
    use std::fs;
    use std::path::{Path, PathBuf};
    use std::time::{SystemTime, UNIX_EPOCH};

    use super::*;

    struct LocalRepo {
        root: PathBuf,
        repo: Repo,
    }

    impl LocalRepo {
        fn new(name: &str) -> Self {
            let root = local_workspace(name);
            let repo = init(&root).expect("initialize local git repository");
            repo.config_set("user.name", "Test User")
                .expect("set user.name");
            repo.config_set("user.email", "test@example.com")
                .expect("set user.email");
            Self { root, repo }
        }

        fn write(&self, path: &str, content: &str) {
            let full_path = self.root.join(path);
            if let Some(parent) = full_path.parent() {
                fs::create_dir_all(parent).expect("create parent directories");
            }
            fs::write(full_path, content).expect("write repository file");
        }

        fn commit_all(&self, message: &str) -> Oid {
            let paths = self
                .repo
                .status()
                .expect("read status")
                .into_iter()
                .map(|entry| entry.path)
                .collect::<Vec<_>>();
            let refs = paths.iter().map(String::as_str).collect::<Vec<_>>();
            if !refs.is_empty() {
                self.repo.stage(&refs).expect("stage status paths");
            }
            self.repo
                .commit(message, None)
                .expect("commit staged changes")
        }
    }

    impl Drop for LocalRepo {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.root);
        }
    }

    fn local_workspace(name: &str) -> PathBuf {
        let base = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("..")
            .join("..")
            .join("target")
            .join("rskit-git-tests");
        fs::create_dir_all(&base).expect("create local test workspace");
        let unique = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("system time after epoch")
            .as_nanos();
        let root = base.join(format!("{name}-{}-{unique}", std::process::id()));
        let _ = fs::remove_dir_all(&root);
        fs::create_dir_all(&root).expect("create test repo directory");
        root
    }

    #[test]
    fn repository_facade_manages_refs_config_and_remotes_locally() {
        let local = LocalRepo::new("manage");
        local.write("README.md", "# local\n");
        let initial = local.commit_all("initial commit");

        local
            .repo
            .create_branch("feature", "HEAD")
            .expect("create branch");
        local
            .repo
            .create_tag("v1.0.0", "HEAD", "release notes")
            .expect("create annotated tag");
        local
            .repo
            .create_tag("v1.0.1", "HEAD", "")
            .expect("create lightweight tag");
        local
            .repo
            .exec(&[
                "remote",
                "add",
                "origin",
                &format!("file://{}", local.root.display()),
            ])
            .expect("add local remote");

        let branches = local
            .repo
            .list_branches(BranchFilter::All)
            .expect("list branches");
        assert!(branches.iter().any(|branch| branch.name == "feature"));

        let tags = local.repo.list_tags().expect("list tags");
        let annotated = tags
            .iter()
            .find(|tag| tag.name == "v1.0.0")
            .expect("annotated tag exists");
        assert_eq!(annotated.message, "release notes");
        assert_eq!(annotated.target, initial);
        assert!(
            tags.iter()
                .any(|tag| tag.name == "v1.0.1" && tag.message.is_empty())
        );

        let remotes = local.repo.list_remotes().expect("list remotes");
        assert_eq!(remotes[0].name, "origin");
        assert!(remotes[0].url.starts_with("file://"));

        assert_eq!(
            local.repo.config_get("user.email").expect("get config"),
            "test@example.com"
        );
        let missing = local
            .repo
            .config_get("rskit.missing")
            .expect_err("missing config maps to error");
        assert_eq!(missing.code(), rskit_errors::ErrorCode::NotFound);

        let duplicate = local
            .repo
            .create_branch("feature", "HEAD")
            .expect_err("duplicate branch maps to conflict");
        assert_eq!(duplicate.code(), rskit_errors::ErrorCode::AlreadyExists);
    }

    #[test]
    fn repository_facade_handles_index_checkout_stash_and_clean() {
        let local = LocalRepo::new("write");
        local.write("tracked.txt", "one\n");
        local.commit_all("initial commit");

        local.write("tracked.txt", "two\n");
        local
            .repo
            .stage(&["tracked.txt"])
            .expect("stage tracked file");
        assert_eq!(
            local.repo.staged_entries().expect("list staged entries")[0].state,
            EntryState::Staged
        );

        local
            .repo
            .reset("HEAD", ResetMode::Mixed)
            .expect("mixed reset");
        assert!(
            local
                .repo
                .staged_entries()
                .expect("list staged entries after reset")
                .is_empty()
        );

        local
            .repo
            .checkout_files(&["tracked.txt"])
            .expect("restore tracked file");
        assert_eq!(
            fs::read_to_string(local.root.join("tracked.txt")).expect("read restored file"),
            "one\n"
        );

        local.write("tracked.txt", "stashed\n");
        let stash_oid = local.repo.stash("work in progress").expect("create stash");
        assert!(!stash_oid.is_zero());
        let stashes = local.repo.stash_list().expect("list stashes");
        assert_eq!(stashes[0].index, 0);
        assert!(stashes[0].message.contains("work in progress"));
        local.repo.stash_pop().expect("pop stash");
        assert_eq!(
            fs::read_to_string(local.root.join("tracked.txt")).expect("read popped file"),
            "stashed\n"
        );

        local.write("scratch/file.txt", "remove me\n");
        let dry_run = local
            .repo
            .clean(Some(&CleanOptions {
                directories: true,
                ..Default::default()
            }))
            .expect("dry-run clean");
        assert!(dry_run.iter().any(|path| path == "scratch/"));
        local
            .repo
            .clean(Some(&CleanOptions {
                directories: true,
                force: true,
                ..Default::default()
            }))
            .expect("force clean");
        assert!(!local.root.join("scratch").exists());
    }

    #[test]
    fn commits_support_explicit_signatures_and_amend_without_signing() {
        let local = LocalRepo::new("commit-options");
        local.write("file.txt", "one\n");
        local.repo.stage(&["file.txt"]).expect("stage file");
        let signature = Signature {
            name: "Before Epoch".to_string(),
            email: "before@example.com".to_string(),
            when: UNIX_EPOCH - std::time::Duration::from_secs(60),
        };
        let first = local
            .repo
            .commit(
                "initial",
                Some(&CommitOptions {
                    author: Some(signature.clone()),
                    committer: Some(signature),
                    ..Default::default()
                }),
            )
            .expect("commit with explicit signature");
        assert!(!first.is_zero());

        local.write("file.txt", "two\n");
        local.repo.stage(&["file.txt"]).expect("stage amendment");
        let amended = local
            .repo
            .commit(
                "amended",
                Some(&CommitOptions {
                    amend: true,
                    ..Default::default()
                }),
            )
            .expect("amend commit");
        assert_ne!(first, amended);

        let signing = local
            .repo
            .commit(
                "signed",
                Some(&CommitOptions {
                    sign: true,
                    ..Default::default()
                }),
            )
            .expect_err("signing is unsupported");
        assert_eq!(signing.code(), rskit_errors::ErrorCode::InvalidInput);
    }
}
