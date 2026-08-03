#![allow(dead_code)]

use std::path::{Path, PathBuf};

use rskit_process::{ProcessConfig, ProcessSpec, run};
use tempfile::TempDir;

pub struct TestRepo {
    pub dir: TempDir,
}

impl TestRepo {
    pub fn init() -> Self {
        let dir = TempDir::new().expect("failed to create temp dir");
        run_git(dir.path(), &["init"]);
        run_git(dir.path(), &["config", "user.email", "test@test.com"]);
        run_git(dir.path(), &["config", "user.name", "Test User"]);
        write_file(dir.path(), "README.md", "# test repo");
        run_git(dir.path(), &["add", "."]);
        run_git(dir.path(), &["commit", "-m", "initial commit"]);
        Self { dir }
    }

    pub fn init_bare() -> Self {
        let dir = TempDir::new().expect("failed to create temp dir");
        run_git(dir.path(), &["init", "--bare"]);
        Self { dir }
    }

    pub fn empty_dir() -> Self {
        let dir = TempDir::new().expect("failed to create temp dir");
        Self { dir }
    }

    pub fn path(&self) -> &Path {
        self.dir.path()
    }

    pub fn commit_file(&self, path: &str, content: &str, message: &str) {
        write_file(self.path(), path, content);
        run_git(self.path(), &["add", path]);
        run_git(self.path(), &["commit", "-m", message]);
    }

    pub fn create_branch(&self, name: &str) {
        run_git(self.path(), &["branch", name]);
    }

    pub fn add_remote(&self, name: &str, url: &str) {
        run_git(self.path(), &["remote", "add", name, url]);
    }

    pub fn push_upstream(&self, remote: &str, branch: &str) {
        run_git(self.path(), &["push", "-u", remote, branch]);
    }

    pub fn create_tag(&self, name: &str) {
        run_git(self.path(), &["tag", name]);
    }

    /// Runs an arbitrary git command in the repo and returns stdout.
    pub fn git(&self, args: &[&str]) -> String {
        run_git(self.path(), args)
    }

    pub fn create_annotated_tag(&self, name: &str, message: &str) {
        run_git(self.path(), &["tag", "-a", name, "-m", message]);
    }

    pub fn checkout(&self, name: &str) {
        run_git(self.path(), &["checkout", name]);
    }

    pub fn current_branch(&self) -> String {
        run_git(self.path(), &["branch", "--show-current"])
            .trim()
            .to_string()
    }

    pub fn rev_parse(&self, revision: &str) -> String {
        run_git(self.path(), &["rev-parse", revision])
            .trim()
            .to_string()
    }

    pub fn make_dirty(&self, path: &str) {
        write_file(self.path(), path, "dirty content\n");
    }

    pub fn make_untracked(&self, path: &str) {
        write_file(self.path(), path, "untracked\n");
    }

    pub fn config_set(&self, key: &str, value: &str) {
        run_git(self.path(), &["config", key, value]);
    }

    /// Generates a passphrase-less ed25519 SSH key inside the repo and returns
    /// its private-key path, suitable for `user.signingkey` with `gpg.format=ssh`.
    /// SSH signing works offline and deterministically, so it exercises the real
    /// signing path without a GPG keyring or network.
    pub fn create_ssh_signing_key(&self) -> PathBuf {
        let key_path = self.path().join("signing_key");
        let key_arg = key_path.to_str().expect("utf-8 key path");
        let cmd = ProcessSpec::new("ssh-keygen")
            .args([
                "-q",
                "-t",
                "ed25519",
                "-N",
                "",
                "-C",
                "rskit-git-test",
                "-f",
                key_arg,
            ])
            .dir(self.path());
        let out = run(&cmd, &ProcessConfig::default().with_timeout(None))
            .expect("failed to run ssh-keygen");
        if !out.success() {
            panic!("ssh-keygen failed: {}", out.stderr);
        }
        key_path
    }

    pub fn config_add(&self, key: &str, value: &str) {
        run_git(self.path(), &["config", "--add", key, value]);
    }
}

fn write_file(dir: &Path, path: &str, content: &str) {
    let full = dir.join(path);
    if let Some(parent) = full.parent() {
        std::fs::create_dir_all(parent).expect("failed to create dirs");
    }
    std::fs::write(&full, content).expect("failed to write file");
}

fn run_git(dir: &Path, args: &[&str]) -> String {
    let cmd = ProcessSpec::new("git").args(args.iter().copied()).dir(dir);
    let out = run(&cmd, &ProcessConfig::default().with_timeout(None)).expect("failed to run git");
    if !out.success() {
        panic!("git {:?} failed: {}", args, out.stderr);
    }
    out.stdout
}
