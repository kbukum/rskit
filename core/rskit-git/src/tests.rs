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

/// Point every ambient git config level at an empty directory so tests never
/// inherit the developer's global `user.name` / `user.email`.
///
/// libgit2 exposes config search paths only as a process-global setting, so this
/// runs exactly once per process. Every other test configures identity at the
/// local level, which takes precedence, so the isolation is harmless for them
/// and simply makes the suite hermetic and CI-faithful.
fn ensure_isolated_git_config() {
    use std::sync::OnceLock;

    static ISOLATED: OnceLock<()> = OnceLock::new();
    ISOLATED.get_or_init(|| {
        let empty = local_workspace("empty-git-config");
        for level in [
            git2::ConfigLevel::System,
            git2::ConfigLevel::Global,
            git2::ConfigLevel::XDG,
            git2::ConfigLevel::ProgramData,
        ] {
            // SAFETY: libgit2 search-path configuration is process-global. It is
            // set once, before any identity-sensitive test reads config, and only
            // ever redirects config discovery at an empty directory — never at a
            // path that could grant an unexpected identity.
            unsafe {
                let _ = git2::opts::set_search_path(level, &empty);
            }
        }
    });
}

#[test]
fn repository_facade_manages_refs_config_and_remotes_locally() {
    let local = LocalRepo::new("manage");
    local.write(
        "README.md",
        "# local
",
    );
    let initial = local.commit_all("initial commit");

    local
        .repo
        .create_branch("feature", "HEAD")
        .expect("create branch");
    local
        .repo
        .create_tag("v1.0.0", "HEAD", Some("release notes"))
        .expect("create annotated tag");
    local
        .repo
        .create_tag("v1.0.1", "HEAD", None)
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
    local.write(
        "tracked.txt",
        "one
",
    );
    local.commit_all("initial commit");

    local.write(
        "tracked.txt",
        "two
",
    );
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
        "one
"
    );

    local.write(
        "tracked.txt",
        "stashed
",
    );
    let stash_oid = local.repo.stash("work in progress").expect("create stash");
    assert!(!stash_oid.is_zero());
    let stashes = local.repo.stash_list().expect("list stashes");
    assert_eq!(stashes[0].index, 0);
    assert!(stashes[0].message.contains("work in progress"));
    local.repo.stash_pop().expect("pop stash");
    assert_eq!(
        fs::read_to_string(local.root.join("tracked.txt")).expect("read popped file"),
        "stashed
"
    );

    local.write(
        "scratch/file.txt",
        "remove me
",
    );
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
    local.write(
        "file.txt", "one
",
    );
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

    local.write(
        "file.txt", "two
",
    );
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

#[test]
fn commit_without_configured_identity_yields_actionable_error() {
    ensure_isolated_git_config();

    let root = local_workspace("missing-identity");
    let repo = init(&root).expect("initialize local git repository");
    // Intentionally leave user.name / user.email unset — mirroring a fresh CI
    // checkout that authenticates but never configures a git identity.

    fs::write(
        root.join("file.txt"),
        "one
",
    )
    .expect("write repository file");
    repo.stage(&["file.txt"]).expect("stage file");

    let error = repo
        .commit("initial", None)
        .expect_err("commit without identity must fail");

    assert_eq!(error.code(), rskit_errors::ErrorCode::InvalidInput);
    let message = error.message();
    assert!(
        message.contains("git config user.name") && message.contains("git config user.email"),
        "expected actionable identity guidance, got: {message}"
    );
    assert!(
        !message.contains("internal server error"),
        "identity failure must not surface as an opaque internal error: {message}"
    );
}
