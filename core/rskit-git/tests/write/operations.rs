use crate::helpers;

use std::time::{Duration, UNIX_EPOCH};

use rskit_git::{
    CheckoutManager, CheckoutOptions, CherryPickOptions, CherryPicker, CommitOptions, Committer,
    Differ, EntryState, IgnoreReader, IndexManager, LogOptions, LogReader, MergeOptions, Merger,
    RebaseOptions, Rebaser, Repository, ResetMode, Resetter, Signature, Stasher, cli::GitCli, open,
};

#[test]
fn stage_adds_tracked_and_untracked_files_to_index() {
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
fn unstage_removes_selected_file_from_index() {
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
fn staged_entries_returns_index_state() {
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
fn commit_with_message_updates_head() {
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
fn commit_with_options_preserves_author_and_committer_metadata() {
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
fn commit_amend_preserves_parent_chain() {
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
fn merge_checkout_and_reset_update_worktree_state() {
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
fn stash_push_list_and_pop_round_trips_dirty_worktree() {
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
fn rebase_and_cherry_pick_apply_commits_without_conflicts() {
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

#[test]
fn cli_backend_covers_option_rich_write_operations() {
    let repo = helpers::TestRepo::init();
    let main_branch = repo.current_branch();
    let cli = GitCli::new(repo.path().to_path_buf());

    cli.checkout(
        "HEAD",
        Some(&CheckoutOptions {
            create_branch: Some("cli-feature".to_string()),
            force: true,
            ..Default::default()
        }),
    )
    .unwrap();
    repo.commit_file("cli-feature.txt", "feature\n", "cli feature");
    let feature_commit = repo.rev_parse("HEAD");

    cli.checkout(&main_branch, None).unwrap();
    repo.commit_file("main.txt", "main\n", "cli main");
    cli.merge(
        "cli-feature",
        Some(&MergeOptions {
            no_fast_forward: true,
            message: Some("merge cli feature".to_string()),
            ..Default::default()
        }),
    )
    .unwrap();
    assert!(repo.path().join("cli-feature.txt").exists());

    cli.reset("HEAD~1", ResetMode::Soft).unwrap();
    cli.reset("HEAD", ResetMode::Mixed).unwrap();
    cli.reset("HEAD", ResetMode::Hard).unwrap();
    let _ = std::fs::remove_file(repo.path().join("cli-feature.txt"));
    cli.checkout(
        &feature_commit,
        Some(&CheckoutOptions {
            detach: true,
            ..Default::default()
        }),
    )
    .unwrap();
    cli.checkout(&main_branch, None).unwrap();

    cli.checkout(
        "HEAD",
        Some(&CheckoutOptions {
            create_branch: Some("cli-rebase".to_string()),
            ..Default::default()
        }),
    )
    .unwrap();
    repo.commit_file("rebase.txt", "rebase\n", "cli rebase");
    cli.rebase(
        &main_branch,
        Some(&RebaseOptions {
            autosquash: true,
            ..Default::default()
        }),
    )
    .unwrap();

    cli.checkout(&main_branch, None).unwrap();
    cli.cherry_pick(
        &feature_commit,
        Some(&CherryPickOptions {
            no_commit: true,
            ..Default::default()
        }),
    )
    .unwrap();
    assert!(repo.path().join("cli-feature.txt").exists());
    cli.reset("HEAD", ResetMode::Hard).unwrap();

    std::fs::write(repo.path().join("README.md"), "stash one\n").unwrap();
    let first = cli.stash("first").unwrap();
    std::fs::write(repo.path().join("README.md"), "stash two\n").unwrap();
    let second = cli.stash("second").unwrap();
    let stashes = cli.stash_list().unwrap();
    assert_eq!(stashes.len(), 2);
    assert!(stashes.iter().any(|stash| stash.oid == first));
    assert!(stashes.iter().any(|stash| stash.oid == second));
    cli.stash_pop_index(1).unwrap();
}

#[test]
fn facade_covers_aliases_and_abort_paths_without_active_operations() {
    let repo = helpers::TestRepo::init();
    std::fs::write(repo.path().join(".gitignore"), "*.ignored\n").unwrap();
    std::fs::write(repo.path().join("file.ignored"), "ignored\n").unwrap();
    let r = open(repo.path()).unwrap();

    assert!(r.is_ignored("file.ignored").unwrap());
    assert!(r.merge_abort().is_err());
    assert!(r.rebase_abort().is_err());
    assert!(r.rebase_continue().is_err());
    assert!(r.cherry_pick_abort().is_err());
    assert!(r.cherry_pick_continue().is_err());

    std::fs::write(repo.path().join("README.md"), "alias stash\n").unwrap();
    let oid = r.stash_push("alias").unwrap();
    assert!(!oid.is_zero());
    r.stash_pop().unwrap();
}
