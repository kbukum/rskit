use crate::helpers;

use std::path::Path;
use std::process::Command;
use std::time::{Duration, UNIX_EPOCH};

use rskit_errors::ErrorCode;
use rskit_git::auth::TransportAuth;
use rskit_git::{
    BranchFilter, CommitOptions, Committer, ConfigReader, EntryState, FetchOptions, IndexManager,
    LogOptions, LogReader, PushOptions, RefManager, RemoteManager, Repository, Signature, open,
};

#[test]
fn unborn_head_maps_to_ref_not_found() {
    let repo = helpers::TestRepo::empty_dir();
    let r = rskit_git::init(repo.path()).unwrap();

    let err = r.head().unwrap_err();

    assert_eq!(err.code(), ErrorCode::NotFound);
    assert!(err.message().contains("HEAD"));
}

#[test]
fn branch_filters_include_remote_refs_without_upstreams() {
    let source = helpers::TestRepo::init();
    let remote = helpers::TestRepo::init_bare();
    let clone_dir = helpers::TestRepo::empty_dir();
    let branch = source.current_branch();
    source.add_remote("origin", remote.path().to_str().unwrap());
    source.push_upstream("origin", &branch);
    let clone = rskit_git::clone(remote.path().to_str().unwrap(), clone_dir.path()).unwrap();

    let remote_branches = clone.list_branches(BranchFilter::Remote).unwrap();
    let all_branches = clone.list_branches(BranchFilter::All).unwrap();

    assert!(
        remote_branches
            .iter()
            .any(|entry| entry.name == format!("origin/{branch}") && entry.upstream.is_none())
    );
    assert!(all_branches.iter().any(|entry| entry.name == branch));
    assert!(
        all_branches
            .iter()
            .any(|entry| entry.name == format!("origin/{branch}"))
    );
}

#[test]
fn invalid_ref_targets_and_duplicate_refs_return_typed_errors() {
    let repo = helpers::TestRepo::init();
    let r = open(repo.path()).unwrap();

    let missing_target = r
        .create_tag("missing-target", "refs/missing", None)
        .unwrap_err();
    assert_eq!(missing_target.code(), ErrorCode::NotFound);
    assert!(missing_target.message().contains("refs/missing"));

    r.create_branch("feature", "HEAD").unwrap();
    let duplicate_branch = r.create_branch("feature", "HEAD").unwrap_err();
    assert_eq!(duplicate_branch.code(), ErrorCode::AlreadyExists);
    assert!(duplicate_branch.message().contains("branch 'feature'"));

    r.create_tag("v1", "HEAD", None).unwrap();
    let duplicate_tag = r.create_tag("v1", "HEAD", None).unwrap_err();
    assert_eq!(duplicate_tag.code(), ErrorCode::AlreadyExists);
    assert!(duplicate_tag.message().contains("tag 'v1'"));
}

#[test]
fn invalid_ref_names_map_unexpected_git2_errors_to_internal() {
    let repo = helpers::TestRepo::init();
    let r = open(repo.path()).unwrap();

    let invalid_branch = r.create_branch("bad branch name", "HEAD").unwrap_err();
    assert_eq!(invalid_branch.code(), ErrorCode::Internal);

    let invalid_tag = r.create_tag("bad tag name", "HEAD", None).unwrap_err();
    assert_eq!(invalid_tag.code(), ErrorCode::Internal);
}

#[test]
fn current_branch_delete_and_tracking_failures_are_typed() {
    let repo = helpers::TestRepo::init();
    let r = open(repo.path()).unwrap();
    let current = repo.current_branch();

    let checked_out = r.delete_branch(&current).unwrap_err();
    assert_eq!(checked_out.code(), ErrorCode::Internal);

    let missing_branch = r.tracking_branch("does-not-exist").unwrap_err();
    assert_eq!(missing_branch.code(), ErrorCode::NotFound);
    assert!(missing_branch.message().contains("does-not-exist"));

    let missing_upstream = r.tracking_branch(&current).unwrap_err();
    assert_eq!(missing_upstream.code(), ErrorCode::NotFound);
    assert!(missing_upstream.message().contains("@{upstream}"));
}

#[test]
fn remotes_include_push_specs_and_missing_remote_errors_are_typed() {
    let repo = helpers::TestRepo::init();
    let remote = helpers::TestRepo::init_bare();
    repo.add_remote("origin", remote.path().to_str().unwrap());
    repo.config_add("remote.origin.push", "refs/heads/master:refs/heads/master");
    let r = open(repo.path()).unwrap();

    let remotes = r.list_remotes().unwrap();
    assert_eq!(remotes[0].name, "origin");
    assert!(
        remotes[0]
            .push_specs
            .iter()
            .any(|spec| spec == "refs/heads/master:refs/heads/master")
    );

    let missing_fetch = r.fetch("missing", None).unwrap_err();
    assert_eq!(missing_fetch.code(), ErrorCode::NotFound);
    assert!(missing_fetch.message().contains("missing"));

    let missing_push = r.push("missing", None).unwrap_err();
    assert_eq!(missing_push.code(), ErrorCode::NotFound);
    assert!(missing_push.message().contains("missing"));
}

#[test]
fn fetch_options_validate_depth_before_local_fetch() {
    let repo = helpers::TestRepo::init();
    let remote = helpers::TestRepo::init_bare();
    repo.add_remote("origin", remote.path().to_str().unwrap());
    let r = open(repo.path()).unwrap();

    let err = r
        .fetch(
            "origin",
            Some(&FetchOptions {
                depth: Some(usize::MAX),
                ..Default::default()
            }),
        )
        .unwrap_err();

    assert_eq!(err.code(), ErrorCode::InvalidInput);
    assert!(err.message().contains("depth"));
}

#[test]
fn push_uses_configured_refspecs_and_force_prefixes_defaults() {
    let repo = helpers::TestRepo::init();
    let remote = helpers::TestRepo::init_bare();
    let branch = repo.current_branch();
    repo.add_remote("origin", remote.path().to_str().unwrap());
    repo.config_add(
        "remote.origin.push",
        &format!("refs/heads/{branch}:refs/heads/{branch}"),
    );
    let r = open(repo.path()).unwrap();

    r.push(
        "origin",
        Some(&PushOptions {
            force: true,
            ..Default::default()
        }),
    )
    .unwrap();

    let pushed = open(remote.path()).unwrap().resolve_ref(&branch).unwrap();
    assert!(!pushed.is_zero());
}

#[test]
fn non_fast_forward_push_is_a_typed_rejection_not_internal() {
    // Publish an initial branch to a shared bare remote.
    let source = helpers::TestRepo::init();
    let remote = helpers::TestRepo::init_bare();
    let branch = source.current_branch();
    source.add_remote("origin", remote.path().to_str().unwrap());
    source.push_upstream("origin", &branch);

    // A second clone advances the remote branch beyond `source`.
    let other = helpers::TestRepo::empty_dir();
    rskit_git::clone(remote.path().to_str().unwrap(), other.path()).unwrap();
    other.config_set("user.email", "other@test.com");
    other.config_set("user.name", "Other");
    other.commit_file("advance.txt", "remote ahead", "advance remote");
    other.push_upstream("origin", &branch);

    // `source` commits on top of the stale tip, so pushing it is non-fast-forward.
    source.commit_file("local.txt", "local work", "local commit");
    let r = open(source.path()).unwrap();

    let err = r
        .push(
            "origin",
            Some(&PushOptions {
                refspecs: vec![format!("refs/heads/{branch}:refs/heads/{branch}")],
                ..Default::default()
            }),
        )
        .expect_err("non-fast-forward push is rejected");

    assert_eq!(err.code(), ErrorCode::Conflict);
    assert!(err.message().contains(&branch), "{err}");
    assert_ne!(err.message(), "internal server error", "{err}");
}

#[test]
fn push_with_auth_provider_preserves_typed_rejection() {
    // Same non-fast-forward setup, but the repo is opened with an explicit auth
    // provider so the credentials callback is present. The typed `PushRejected`
    // path must still fire, proving the auth callbacks and the rejection
    // recorder are merged onto one callbacks object rather than one clobbering
    // the other.
    let source = helpers::TestRepo::init();
    let remote = helpers::TestRepo::init_bare();
    let branch = source.current_branch();
    source.add_remote("origin", remote.path().to_str().unwrap());
    source.push_upstream("origin", &branch);

    let other = helpers::TestRepo::empty_dir();
    rskit_git::clone(remote.path().to_str().unwrap(), other.path()).unwrap();
    other.config_set("user.email", "other@test.com");
    other.config_set("user.name", "Other");
    other.commit_file("advance.txt", "remote ahead", "advance remote");
    other.push_upstream("origin", &branch);

    source.commit_file("local.txt", "local work", "local commit");

    let auth = std::sync::Arc::new(rskit_git::auth::StaticAuthProvider::new(
        TransportAuth::Token {
            username: Some(rskit_git::auth::DEFAULT_TOKEN_USERNAME.to_string()),
            token: rskit_git::auth::SecretString::new("unused-for-local-transport"),
        },
    ));
    let r = rskit_git::open_with_auth(source.path(), auth).unwrap();

    let err = r
        .push(
            "origin",
            Some(&PushOptions {
                refspecs: vec![format!("refs/heads/{branch}:refs/heads/{branch}")],
                ..Default::default()
            }),
        )
        .expect_err("non-fast-forward push is rejected even with an auth provider");

    assert_eq!(err.code(), ErrorCode::Conflict);
    assert!(err.message().contains(&branch), "{err}");
}

#[test]
fn config_get_all_missing_and_invalid_config_set_return_typed_errors() {
    let repo = helpers::TestRepo::init();
    let r = open(repo.path()).unwrap();

    assert!(r.config_get_all("rskit.missing").unwrap().is_empty());

    let invalid = r.config_set("bad key", "value").unwrap_err();
    assert_eq!(invalid.code(), ErrorCode::Internal);
}

#[test]
fn stage_rejects_bare_repositories_and_invalid_paths() {
    let bare = helpers::TestRepo::init_bare();
    let bare_repo = open(bare.path()).unwrap();

    let bare_err = bare_repo.stage(&["file.txt"]).unwrap_err();
    assert_eq!(bare_err.code(), ErrorCode::InvalidInput);
    assert!(bare_err.message().contains("stage on bare repository"));

    let repo = helpers::TestRepo::init();
    let r = open(repo.path()).unwrap();
    let parent = r.stage(&["../outside.txt"]).unwrap_err();
    assert_eq!(parent.code(), ErrorCode::InvalidInput);
    assert!(parent.message().contains("../outside.txt"));

    let absolute = r
        .stage(&[repo.path().join("README.md").to_str().unwrap()])
        .unwrap_err();
    assert_eq!(absolute.code(), ErrorCode::InvalidInput);
}

#[test]
fn stage_deleted_file_and_empty_unstage_are_noops_for_expected_paths() {
    let repo = helpers::TestRepo::init();
    let r = open(repo.path()).unwrap();

    r.unstage(&[]).unwrap();
    std::fs::remove_file(repo.path().join("README.md")).unwrap();
    r.stage(&["README.md"]).unwrap();

    let staged = r.staged_entries().unwrap();
    assert!(
        staged
            .iter()
            .any(|entry| entry.path == "README.md" && entry.state == EntryState::Staged)
    );
}

#[test]
fn staged_entries_report_conflicted_index_entries() {
    let repo = helpers::TestRepo::init();
    let main = repo.current_branch();
    run_git(repo.path(), &["checkout", "-b", "feature"]);
    std::fs::write(repo.path().join("README.md"), "feature\n").unwrap();
    run_git(repo.path(), &["add", "README.md"]);
    run_git(repo.path(), &["commit", "-m", "feature change"]);
    run_git(repo.path(), &["checkout", &main]);
    std::fs::write(repo.path().join("README.md"), "main\n").unwrap();
    run_git(repo.path(), &["add", "README.md"]);
    run_git(repo.path(), &["commit", "-m", "main change"]);

    let merge = Command::new("git")
        .args(["merge", "feature"])
        .current_dir(repo.path())
        .output()
        .expect("run conflicting merge");
    assert!(!merge.status.success());
    let r = open(repo.path()).unwrap();

    let staged = r.staged_entries().unwrap();

    assert!(
        staged
            .iter()
            .any(|entry| entry.path == "README.md" && entry.state == EntryState::Conflicted)
    );
}

#[test]
fn commit_rejects_signing_and_supports_negative_author_times() {
    let repo = helpers::TestRepo::init();
    let r = open(repo.path()).unwrap();

    let signed = r
        .commit(
            "signed",
            Some(&CommitOptions {
                sign: true,
                ..Default::default()
            }),
        )
        .unwrap_err();
    assert_eq!(signed.code(), ErrorCode::InvalidInput);
    assert!(signed.message().contains("signing"));

    std::fs::write(repo.path().join("README.md"), "before epoch\n").unwrap();
    r.stage(&["README.md"]).unwrap();
    let oid = r
        .commit(
            "before epoch",
            Some(&CommitOptions {
                author: Some(Signature {
                    name: "Old Author".to_string(),
                    email: "old@example.com".to_string(),
                    when: UNIX_EPOCH - Duration::from_secs(1),
                }),
                committer: Some(Signature {
                    name: "Old Committer".to_string(),
                    email: "committer@example.com".to_string(),
                    when: UNIX_EPOCH - Duration::from_secs(1),
                }),
                ..Default::default()
            }),
        )
        .unwrap();

    let commit = r
        .log(Some(&LogOptions {
            max_count: Some(1),
            ..Default::default()
        }))
        .unwrap()
        .remove(0);
    assert_eq!(commit.oid, oid);
    assert_eq!(commit.author.name, "Old Author");
}

#[test]
fn amend_uses_explicit_author_and_committer() {
    let repo = helpers::TestRepo::init();
    let r = open(repo.path()).unwrap();

    std::fs::write(repo.path().join("README.md"), "amended\n").unwrap();
    r.stage(&["README.md"]).unwrap();
    let oid = r
        .commit(
            "amended",
            Some(&CommitOptions {
                amend: true,
                author: Some(Signature {
                    name: "Amend Author".to_string(),
                    email: "amend@example.com".to_string(),
                    when: UNIX_EPOCH + Duration::from_secs(5),
                }),
                committer: Some(Signature {
                    name: "Amend Committer".to_string(),
                    email: "amend-committer@example.com".to_string(),
                    when: UNIX_EPOCH + Duration::from_secs(6),
                }),
                ..Default::default()
            }),
        )
        .unwrap();

    let commit = r
        .log(Some(&LogOptions {
            max_count: Some(1),
            ..Default::default()
        }))
        .unwrap()
        .remove(0);
    assert_eq!(commit.oid, oid);
    assert_eq!(commit.author.name, "Amend Author");
    assert_eq!(commit.committer.name, "Amend Committer");
}

#[test]
fn embedded_auth_callbacks_validate_without_network() {
    let variants = [
        TransportAuth::Default,
        TransportAuth::UsernamePassword {
            username: "user".to_string(),
            password: rskit_git::auth::SecretString::new("password"),
        },
        TransportAuth::Token {
            username: None,
            token: rskit_git::auth::SecretString::new("token"),
        },
        TransportAuth::SshKey {
            username: "git".to_string(),
            public_key: None,
            private_key: repo_relative_key("id_ed25519"),
            passphrase: Some(rskit_git::auth::SecretString::new("passphrase")),
        },
        TransportAuth::SshAgent {
            username: "git".to_string(),
        },
    ];

    let _default_callbacks = rskit_git::embedded::auth::remote_callbacks(None).unwrap();
    for auth in variants {
        let _callbacks = rskit_git::embedded::auth::remote_callbacks(Some(&auth)).unwrap();
    }
}

fn repo_relative_key(path: &str) -> std::path::PathBuf {
    Path::new(path).to_path_buf()
}

fn run_git(dir: &Path, args: &[&str]) -> String {
    let output = Command::new("git")
        .args(args)
        .current_dir(dir)
        .output()
        .expect("run git");
    assert!(
        output.status.success(),
        "git {args:?} failed: stdout={}, stderr={}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    String::from_utf8(output.stdout).expect("git stdout is utf-8")
}
