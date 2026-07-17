use crate::helpers;

use std::path::Path;
use std::time::{Duration, UNIX_EPOCH};

use rskit_errors::ErrorCode;
use rskit_git::{
    BlameOptions, Blamer, Differ, EntryKind, EntryState, FileStatus, IndexReader, LogReader,
    Repository, TreeReader, discover, init, open,
};
use rskit_process::{ProcessConfig, ProcessSpec, run};

#[test]
fn repository_open_discover_and_head_edge_errors_are_typed() {
    let empty = helpers::TestRepo::empty_dir();

    let missing = open(empty.path().join("missing"))
        .err()
        .expect("missing path is not found");
    assert_eq!(missing.code(), ErrorCode::NotFound);
    assert!(missing.message().contains("repository"));

    let not_a_repo = open(empty.path())
        .err()
        .expect("plain directory is not a repository");
    assert_eq!(not_a_repo.code(), ErrorCode::NotFound);
    assert!(not_a_repo.message().contains("repository"));

    let undiscovered = discover(empty.path())
        .err()
        .expect("no parent repository exists");
    assert_eq!(undiscovered.code(), ErrorCode::NotFound);
    assert!(undiscovered.message().contains("repository"));

    let unborn = init(empty.path().join("repo")).expect("init empty repo");
    let head = unborn.head().expect_err("unborn HEAD has no target");
    assert_eq!(head.code(), ErrorCode::NotFound);
}

#[test]
fn bare_and_detached_heads_report_roots_and_references() {
    let bare = helpers::TestRepo::init_bare();
    let bare_repo = open(bare.path()).expect("open bare repo");
    assert_eq!(
        bare_repo.root().canonicalize().unwrap(),
        bare.path().canonicalize().unwrap()
    );

    let repo = helpers::TestRepo::init();
    let head = repo.rev_parse("HEAD");
    run_git(repo.path(), &["checkout", "--detach", &head]);
    let r = open(repo.path()).expect("open detached repo");

    let detached = r.head().expect("detached HEAD still resolves");
    assert_eq!(detached.name, "HEAD");
    assert!(!detached.is_branch);
    assert_eq!(detached.target.to_string(), head);
}

#[test]
fn invalid_revisions_and_tree_paths_return_typed_errors() {
    let repo = helpers::TestRepo::init();
    let r = open(repo.path()).unwrap();
    let tree = r.tree_hash("HEAD", "").unwrap().to_string();
    let blob = r
        .index_entry("README.md")
        .unwrap()
        .expect("README.md is indexed")
        .oid
        .to_string();

    assert_not_found(r.file_at("missing-ref", "README.md"), "missing-ref");
    assert_not_found(r.file_at(&tree, "README.md"), &tree);
    assert_invalid_input(r.tree_hash("HEAD", "README.md"), "README.md");
    assert_invalid_input(r.list_entries("HEAD", "missing-dir"), "missing-dir");
    assert_not_found(r.list_entries(&blob, ""), &blob);
    assert_not_found(r.diff("missing-ref", "HEAD"), "missing-ref");
    assert_not_found(r.diff(&blob, "HEAD"), &blob);
    assert_not_found(r.merge_base("missing-ref", "HEAD"), "missing-ref");
    assert_not_found(r.merge_base(&tree, "HEAD"), &tree);
    assert_not_found(r.resolve_ref("refs/heads/does-not-exist"), "does-not-exist");
}

#[test]
fn diff_status_and_tree_entries_cover_deleted_conflicted_and_submodule_states() {
    let repo = helpers::TestRepo::init();
    repo.commit_file("deleted.txt", "gone\n", "add deleted file");
    repo.create_tag("before-delete");
    std::fs::remove_file(repo.path().join("deleted.txt")).unwrap();
    run_git(repo.path(), &["add", "deleted.txt"]);
    run_git(repo.path(), &["commit", "-m", "delete file"]);

    let r = open(repo.path()).unwrap();
    let diff = r.diff("before-delete", "HEAD").unwrap();
    assert!(
        diff.iter()
            .any(|entry| entry.path == "deleted.txt" && entry.status == FileStatus::Deleted)
    );

    let submodule_oid = repo.rev_parse("HEAD");
    run_git(
        repo.path(),
        &[
            "update-index",
            "--add",
            "--cacheinfo",
            &format!("160000,{submodule_oid},vendor/lib"),
        ],
    );
    run_git(repo.path(), &["commit", "-m", "add gitlink"]);
    let r = open(repo.path()).unwrap();
    let entries = r.list_entries("HEAD", "vendor").unwrap();
    assert!(
        entries
            .iter()
            .any(|entry| entry.name == "lib" && entry.kind == EntryKind::Submodule)
    );

    let conflict = helpers::TestRepo::init();
    conflict.commit_file("conflict.txt", "base\n", "add conflict base");
    conflict.create_branch("feature");
    let main = conflict.current_branch();
    conflict.checkout("feature");
    conflict.commit_file("conflict.txt", "feature\n", "feature edit");
    conflict.checkout(&main);
    conflict.commit_file("conflict.txt", "main\n", "main edit");
    let merge = run_git_allow_failure(conflict.path(), &["merge", "feature"]);
    assert!(!merge.success(), "merge should conflict");
    let conflicted = open(conflict.path()).unwrap().status().unwrap();
    assert!(
        conflicted
            .iter()
            .any(|entry| entry.path == "conflict.txt" && entry.state == EntryState::Conflicted)
    );
}

#[test]
fn log_blame_and_merge_base_edge_cases_are_typed() {
    let empty = init(helpers::TestRepo::empty_dir().path()).expect("init empty repo");
    let log = empty.log(None).expect_err("empty repository has no HEAD");
    assert_eq!(log.code(), ErrorCode::Internal);

    let repo = helpers::TestRepo::init();
    repo.commit_file("notes.txt", "one\ntwo\n", "add notes");
    let r = open(repo.path()).unwrap();

    let range = r
        .blame(
            "HEAD",
            "notes.txt",
            Some(&BlameOptions {
                start_line: Some(3),
                end_line: Some(2),
                ..Default::default()
            }),
        )
        .expect_err("invalid blame range is rejected");
    assert_eq!(range.code(), ErrorCode::InvalidInput);
    assert!(range.message().contains("3..2"));

    assert_not_found(r.blame("missing-ref", "notes.txt", None), "missing-ref");

    let disconnected = helpers::TestRepo::init();
    let first = disconnected.rev_parse("HEAD");
    run_git(disconnected.path(), &["checkout", "--orphan", "other-root"]);
    run_git(disconnected.path(), &["rm", "-rf", "."]);
    std::fs::write(disconnected.path().join("other.txt"), "other\n").unwrap();
    run_git(disconnected.path(), &["add", "other.txt"]);
    run_git(disconnected.path(), &["commit", "-m", "other root"]);
    let r = open(disconnected.path()).unwrap();
    let no_base = r
        .merge_base(&first, "HEAD")
        .expect_err("orphan histories have no merge base");
    assert_eq!(no_base.code(), ErrorCode::NotFound);
    assert!(no_base.message().contains("merge base"));
}

#[test]
fn pre_epoch_commit_times_convert_without_panicking() {
    let repo = helpers::TestRepo::init();
    std::fs::write(repo.path().join("old.txt"), "old\n").unwrap();
    commit_with_pre_epoch_time(repo.path(), "old.txt", "pre epoch");

    let r = open(repo.path()).unwrap();
    let commits = r.log(None).unwrap();
    let old = commits
        .iter()
        .find(|commit| commit.message == "pre epoch")
        .expect("pre epoch commit is present");

    assert_eq!(old.author.when, UNIX_EPOCH - Duration::from_secs(5));
    assert_eq!(old.committer.when, UNIX_EPOCH - Duration::from_secs(5));
}

fn assert_not_found<T>(result: rskit_git::AppResult<T>, needle: &str) {
    let err = result
        .err()
        .expect("operation should return a not found error");
    assert_eq!(err.code(), ErrorCode::NotFound);
    assert!(
        err.message().contains(needle),
        "message {:?} did not contain {:?}",
        err.message(),
        needle
    );
}

fn assert_invalid_input<T>(result: rskit_git::AppResult<T>, needle: &str) {
    let err = result
        .err()
        .expect("operation should return an invalid input error");
    assert_eq!(err.code(), ErrorCode::InvalidInput);
    assert!(
        err.message().contains(needle),
        "message {:?} did not contain {:?}",
        err.message(),
        needle
    );
}

fn run_git(dir: &Path, args: &[&str]) -> String {
    let out = run_git_allow_failure(dir, args);
    assert!(out.success(), "git {args:?} failed: {}", out.stderr);
    out.stdout
}

fn run_git_allow_failure(dir: &Path, args: &[&str]) -> rskit_process::ProcessResult {
    let cmd = ProcessSpec::new("git").args(args.iter().copied()).dir(dir);
    run(&cmd, &ProcessConfig::default().with_timeout(None)).expect("failed to run git")
}

fn commit_with_pre_epoch_time(dir: &Path, path: &str, message: &str) {
    let repo = git2::Repository::open(dir).expect("open git2 repository");
    let mut index = repo.index().expect("read index");
    index.add_path(Path::new(path)).expect("add path");
    index.write().expect("write index");
    let tree_oid = index.write_tree().expect("write tree");
    let head_name = repo
        .head()
        .expect("read head")
        .name()
        .expect("head has a name")
        .to_string();
    let parent = repo
        .head()
        .expect("read head")
        .peel_to_commit()
        .expect("peel head to commit");
    let raw = format!(
        "tree {tree_oid}\nparent {}\nauthor Time Traveler <time@example.com> -5 +0000\ncommitter Time Traveler <time@example.com> -5 +0000\n\n{message}\n",
        parent.id()
    );
    let oid = repo
        .odb()
        .expect("open object database")
        .write(git2::ObjectType::Commit, raw.as_bytes())
        .expect("write pre-epoch commit object");
    repo.reference(&head_name, oid, true, "pre-epoch test commit")
        .expect("move branch to pre-epoch commit");
}
