mod helpers;

use std::time::{Duration, UNIX_EPOCH};

use rskit_git::{
    Checker, CheckoutOptions, CherryPicker, CommitOptions, Committer, Differ, EntryState,
    IndexManager, LogOptions, LogReader, Merger, Rebaser, Repository, ResetMode, Resetter,
    Signature, Stasher, open,
};

#[test]
fn test_stage() {
    let repo = helpers::TestRepo::init();
    std::fs::write(repo.path().join("README.md"), "updated\n").unwrap();
    std::fs::write(repo.path().join("new.txt"), "hello\n").unwrap();
    let r = open(repo.path()).unwrap();

    r.stage(&["README.md", "new.txt"]).unwrap();

    let entries = r.staged_entries().unwrap();
    assert!(
        entries
            .iter()
            .any(|entry| entry.path == "README.md" && entry.state == EntryState::Staged)
    );
    assert!(
        entries
            .iter()
            .any(|entry| entry.path == "new.txt" && entry.state == EntryState::Staged)
    );
}

#[test]
fn test_unstage() {
    let repo = helpers::TestRepo::init();
    std::fs::write(repo.path().join("README.md"), "updated\n").unwrap();
    std::fs::write(repo.path().join("new.txt"), "hello\n").unwrap();
    let r = open(repo.path()).unwrap();

    r.stage(&["README.md", "new.txt"]).unwrap();
    r.unstage(&["new.txt"]).unwrap();

    let staged = r.staged_entries().unwrap();
    assert!(
        staged
            .iter()
            .any(|entry| entry.path == "README.md" && entry.state == EntryState::Staged)
    );
    assert!(!staged.iter().any(|entry| entry.path == "new.txt"));

    let status = r.status().unwrap();
    assert!(
        status
            .iter()
            .any(|entry| entry.path == "new.txt" && entry.state == EntryState::Untracked)
    );
}

#[test]
fn test_staged_entries() {
    let repo = helpers::TestRepo::init();
    std::fs::write(repo.path().join("README.md"), "updated\n").unwrap();
    std::fs::write(repo.path().join("untracked.txt"), "pending\n").unwrap();
    let r = open(repo.path()).unwrap();

    r.stage(&["README.md"]).unwrap();

    let entries = r.staged_entries().unwrap();
    assert_eq!(entries.len(), 1);
    assert_eq!(entries[0].path, "README.md");
    assert_eq!(entries[0].state, EntryState::Staged);
}

#[test]
fn test_commit_with_message() {
    let repo = helpers::TestRepo::init();
    std::fs::write(repo.path().join("new.txt"), "hello\n").unwrap();
    let r = open(repo.path()).unwrap();

    r.stage(&["new.txt"]).unwrap();
    let oid = r.commit("add new file", None).unwrap();

    assert!(!oid.is_zero());
    assert_eq!(oid, r.resolve_ref("HEAD").unwrap());

    let commits = r
        .log(Some(&LogOptions {
            max_count: Some(1),
            ..Default::default()
        }))
        .unwrap();
    assert_eq!(commits[0].message, "add new file");
}

#[test]
fn test_commit_with_options() {
    let repo = helpers::TestRepo::init();
    std::fs::write(repo.path().join("README.md"), "updated\n").unwrap();
    let r = open(repo.path()).unwrap();

    r.stage(&["README.md"]).unwrap();
    let opts = CommitOptions {
        author: Some(Signature {
            name: "Author Name".to_string(),
            email: "author@example.com".to_string(),
            when: UNIX_EPOCH + Duration::from_secs(1),
        }),
        committer: Some(Signature {
            name: "Committer Name".to_string(),
            email: "committer@example.com".to_string(),
            when: UNIX_EPOCH + Duration::from_secs(2),
        }),
        ..Default::default()
    };

    let oid = r.commit("custom metadata", Some(&opts)).unwrap();
    let commit = r
        .log(Some(&LogOptions {
            max_count: Some(1),
            ..Default::default()
        }))
        .unwrap()
        .remove(0);

    assert_eq!(oid, commit.oid);
    assert_eq!(commit.author.name, "Author Name");
    assert_eq!(commit.author.email, "author@example.com");
    assert_eq!(commit.committer.name, "Committer Name");
    assert_eq!(commit.committer.email, "committer@example.com");
}

#[test]
fn test_commit_amend_preserves_parent_chain() {
    let repo = helpers::TestRepo::init();
    let base = repo.rev_parse("HEAD");
    let r = open(repo.path()).unwrap();

    std::fs::write(repo.path().join("README.md"), "first update\n").unwrap();
    r.stage(&["README.md"]).unwrap();
    r.commit("first update", None).unwrap();

    std::fs::write(repo.path().join("README.md"), "second update\n").unwrap();
    r.stage(&["README.md"]).unwrap();
    let amended = r
        .commit(
            "amended update",
            Some(&CommitOptions {
                amend: true,
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

    assert_eq!(amended, commit.oid);
    assert_eq!(commit.message, "amended update");
    assert_eq!(commit.parents.len(), 1);
    assert_eq!(commit.parents[0].to_string(), base);
}

#[test]
fn test_merge_checkout_and_reset() {
    let repo = helpers::TestRepo::init();
    let main_branch = repo.current_branch();
    let r = open(repo.path()).unwrap();

    r.checkout(
        "HEAD",
        Some(&CheckoutOptions {
            create_branch: Some("feature".to_string()),
            ..Default::default()
        }),
    )
    .unwrap();
    assert_eq!(repo.current_branch(), "feature");

    repo.commit_file("feature.txt", "feature\n", "feature work");
    r.checkout(&main_branch, None).unwrap();
    let merge = r.merge("feature", None).unwrap();
    assert!(merge.conflicts.is_empty());
    assert_eq!(
        std::fs::read_to_string(repo.path().join("feature.txt")).unwrap(),
        "feature\n"
    );

    std::fs::write(repo.path().join("README.md"), "changed locally\n").unwrap();
    r.checkout_files(&["README.md"]).unwrap();
    assert_eq!(
        std::fs::read_to_string(repo.path().join("README.md")).unwrap(),
        "# test repo"
    );

    repo.commit_file("temp.txt", "temp\n", "temp commit");
    assert!(repo.path().join("temp.txt").exists());
    r.reset("HEAD~1", ResetMode::Hard).unwrap();
    assert!(!repo.path().join("temp.txt").exists());
}

#[test]
fn test_stash_push_list_and_pop() {
    let repo = helpers::TestRepo::init();
    std::fs::write(repo.path().join("README.md"), "dirty content\n").unwrap();
    let r = open(repo.path()).unwrap();

    let stash_oid = r.stash("save readme").unwrap();
    assert!(!stash_oid.is_zero());
    assert_eq!(
        std::fs::read_to_string(repo.path().join("README.md")).unwrap(),
        "# test repo"
    );

    let stashes = r.stash_list().unwrap();
    assert_eq!(stashes.len(), 1);
    assert_eq!(stashes[0].index, 0);
    assert_eq!(stashes[0].oid, stash_oid);
    assert!(stashes[0].message.contains("save readme"));

    r.stash_pop().unwrap();
    assert_eq!(
        std::fs::read_to_string(repo.path().join("README.md")).unwrap(),
        "dirty content\n"
    );
    assert!(r.stash_list().unwrap().is_empty());
}

#[test]
fn test_rebase_and_cherry_pick() {
    let repo = helpers::TestRepo::init();
    let main_branch = repo.current_branch();
    let r = open(repo.path()).unwrap();

    r.checkout(
        "HEAD",
        Some(&CheckoutOptions {
            create_branch: Some("feature".to_string()),
            ..Default::default()
        }),
    )
    .unwrap();
    repo.commit_file("feature.txt", "feature\n", "feature work");
    let feature_commit = repo.rev_parse("HEAD");

    r.checkout(&main_branch, None).unwrap();
    repo.commit_file("main.txt", "main\n", "main work");
    r.checkout("feature", None).unwrap();

    let rebase = r.rebase(&main_branch, None).unwrap();
    assert!(rebase.conflicts.is_empty());
    assert!(rebase.head.is_some());

    r.checkout(&main_branch, None).unwrap();
    let cherry = r.cherry_pick(&feature_commit, None).unwrap();
    assert_eq!(cherry, r.resolve_ref("HEAD").unwrap());
    assert_eq!(
        std::fs::read_to_string(repo.path().join("feature.txt")).unwrap(),
        "feature\n"
    );
}
