use crate::helpers;

use rskit_errors::ErrorCode;
use rskit_git::cli::GitCli;
use rskit_git::types::ResetMode;
use rskit_git::{
    CheckoutManager, CheckoutOptions, CherryPickOptions, CherryPicker, Executor, GrepOptions,
    IgnoreReader, Inspector, MergeOptions, Merger, RebaseOptions, Rebaser, Resetter, Stasher,
};

#[test]
fn cli_merge_reports_conflicts_covers_squash_and_abort_paths() {
    let repo = helpers::TestRepo::init();
    let main_branch = repo.current_branch();
    let cli = GitCli::new(repo.path().to_path_buf());

    cli.checkout(
        "HEAD",
        Some(&CheckoutOptions {
            create_branch: Some("squash-source".to_string()),
            ..Default::default()
        }),
    )
    .unwrap();
    repo.commit_file("squash.txt", "squash\n", "add squash input");
    cli.checkout(&main_branch, None).unwrap();

    let before_squash = repo.rev_parse("HEAD");
    let squash = cli
        .merge(
            "squash-source",
            Some(&MergeOptions {
                squash: true,
                ..Default::default()
            }),
        )
        .unwrap();
    assert_eq!(squash.head.unwrap().to_string(), before_squash);
    assert!(squash.conflicts.is_empty());
    cli.reset("HEAD", ResetMode::Hard).unwrap();

    cli.checkout(
        "HEAD",
        Some(&CheckoutOptions {
            create_branch: Some("conflict-source".to_string()),
            ..Default::default()
        }),
    )
    .unwrap();
    repo.commit_file("conflict.txt", "source\n", "source conflict");
    cli.checkout(&main_branch, None).unwrap();
    repo.commit_file("conflict.txt", "main\n", "main conflict");

    let conflict = cli.merge("conflict-source", None).unwrap();
    assert_eq!(conflict.head, None);
    assert!(!conflict.fast_forward);
    assert_eq!(conflict.conflicts, vec!["conflict.txt"]);

    cli.abort_merge().unwrap();

    let err = cli.checkout("missing-branch", None).unwrap_err();
    assert_eq!(err.code(), ErrorCode::ExternalService);
    assert_eq!(err.message(), "external service error (git)");
    assert!(
        err.cause()
            .expect("preserve git checkout diagnostics")
            .to_string()
            .contains("missing-branch")
    );
}

#[test]
fn cli_rebase_covers_interactive_continue_conflict_success_and_abort_paths() {
    let interactive_repo = helpers::TestRepo::init();
    interactive_repo.config_set("sequence.editor", ":");
    interactive_repo.config_set("core.editor", ":");
    let interactive_main = interactive_repo.current_branch();
    let interactive_cli = GitCli::new(interactive_repo.path().to_path_buf());
    let interactive = interactive_cli
        .rebase(
            &interactive_main,
            Some(&RebaseOptions {
                interactive: true,
                ..Default::default()
            }),
        )
        .unwrap();
    assert!(interactive.head.is_some());
    assert_eq!(interactive.applied, 0);
    assert!(interactive.conflicts.is_empty());

    let repo = helpers::TestRepo::init();
    repo.config_set("core.editor", ":");
    repo.commit_file("a.txt", "base a\n", "base a");
    repo.commit_file("b.txt", "base b\n", "base b");
    let main_branch = repo.current_branch();
    let cli = GitCli::new(repo.path().to_path_buf());

    cli.checkout(
        "HEAD",
        Some(&CheckoutOptions {
            create_branch: Some("rebase-source".to_string()),
            ..Default::default()
        }),
    )
    .unwrap();
    repo.commit_file("a.txt", "source a\n", "source a");
    repo.commit_file("b.txt", "source b\n", "source b");
    cli.checkout(&main_branch, None).unwrap();
    repo.commit_file("a.txt", "main a\n", "main a");
    repo.commit_file("b.txt", "main b\n", "main b");
    cli.checkout("rebase-source", None).unwrap();

    let first_conflict = cli.rebase(&main_branch, None).unwrap();
    assert_eq!(first_conflict.head, None);
    assert_eq!(first_conflict.applied, 0);
    assert_eq!(first_conflict.conflicts, vec!["a.txt"]);

    std::fs::write(repo.path().join("a.txt"), "resolved a\n").unwrap();
    cli.exec(&["add", "a.txt"]).unwrap();
    let second_conflict = cli.continue_rebase().unwrap();
    assert_eq!(second_conflict.head, None);
    assert_eq!(second_conflict.applied, 0);
    assert_eq!(second_conflict.conflicts, vec!["b.txt"]);

    std::fs::write(repo.path().join("b.txt"), "resolved b\n").unwrap();
    cli.exec(&["add", "b.txt"]).unwrap();
    let completed = cli.continue_rebase().unwrap();
    assert!(completed.head.is_some());
    assert!(completed.conflicts.is_empty());

    let abort_repo = helpers::TestRepo::init();
    abort_repo.commit_file("abort.txt", "base\n", "base abort");
    let abort_main = abort_repo.current_branch();
    let abort_cli = GitCli::new(abort_repo.path().to_path_buf());
    abort_cli
        .checkout(
            "HEAD",
            Some(&CheckoutOptions {
                create_branch: Some("abort-source".to_string()),
                ..Default::default()
            }),
        )
        .unwrap();
    abort_repo.commit_file("abort.txt", "source\n", "source abort");
    abort_cli.checkout(&abort_main, None).unwrap();
    abort_repo.commit_file("abort.txt", "main\n", "main abort");
    abort_cli.checkout("abort-source", None).unwrap();
    assert_eq!(
        abort_cli.rebase(&abort_main, None).unwrap().conflicts,
        vec!["abort.txt"]
    );
    abort_cli.abort_rebase().unwrap();
}

#[test]
fn cli_cherry_pick_covers_mainline_continue_and_abort_paths() {
    let repo = helpers::TestRepo::init();
    repo.config_set("core.editor", ":");
    repo.commit_file("pick.txt", "base\n", "base pick");
    let main_branch = repo.current_branch();
    let cli = GitCli::new(repo.path().to_path_buf());

    let mainline_err = cli
        .cherry_pick(
            "HEAD",
            Some(&CherryPickOptions {
                mainline: Some(1),
                ..Default::default()
            }),
        )
        .unwrap_err();
    assert_eq!(mainline_err.code(), ErrorCode::ExternalService);
    assert_eq!(mainline_err.message(), "external service error (git)");
    assert!(
        mainline_err
            .cause()
            .expect("preserve mainline diagnostics")
            .to_string()
            .contains("cherry-pick")
    );

    cli.checkout(
        "HEAD",
        Some(&CheckoutOptions {
            create_branch: Some("pick-source".to_string()),
            ..Default::default()
        }),
    )
    .unwrap();
    repo.commit_file("pick.txt", "source\n", "source pick");
    let source_commit = repo.rev_parse("HEAD");
    cli.checkout(&main_branch, None).unwrap();
    repo.commit_file("pick.txt", "main\n", "main pick");

    let conflict = cli.cherry_pick(&source_commit, None).unwrap_err();
    assert_eq!(conflict.code(), ErrorCode::ExternalService);
    assert!(
        conflict
            .cause()
            .expect("preserve cherry-pick conflict diagnostics")
            .to_string()
            .contains("CONFLICT")
    );
    std::fs::write(repo.path().join("pick.txt"), "resolved\n").unwrap();
    cli.exec(&["add", "pick.txt"]).unwrap();
    let continued = cli.cherry_pick_continue().unwrap();
    assert_eq!(continued.to_string(), repo.rev_parse("HEAD"));

    let abort_repo = helpers::TestRepo::init();
    abort_repo.commit_file("pick.txt", "base\n", "base pick");
    let abort_main = abort_repo.current_branch();
    let abort_cli = GitCli::new(abort_repo.path().to_path_buf());
    abort_cli
        .checkout(
            "HEAD",
            Some(&CheckoutOptions {
                create_branch: Some("abort-pick-source".to_string()),
                ..Default::default()
            }),
        )
        .unwrap();
    abort_repo.commit_file("pick.txt", "source\n", "source pick");
    let abort_source = abort_repo.rev_parse("HEAD");
    abort_cli.checkout(&abort_main, None).unwrap();
    abort_repo.commit_file("pick.txt", "main\n", "main pick");
    assert_eq!(
        abort_cli
            .cherry_pick(&abort_source, None)
            .unwrap_err()
            .code(),
        ErrorCode::ExternalService
    );
    abort_cli.cherry_pick_abort().unwrap();
}

#[test]
fn cli_grep_and_check_ignore_cover_no_line_numbers_no_matches_and_error_exit() {
    let repo = helpers::TestRepo::init();
    repo.commit_file(
        "docs/notes.txt",
        "Alpha\nkey: value: still content\n",
        "add notes",
    );
    let cli = GitCli::new(repo.path().to_path_buf());

    let matches = cli
        .grep(
            "key",
            "HEAD",
            Some(&GrepOptions {
                pathspecs: vec!["docs/notes.txt".to_string()],
                ..Default::default()
            }),
        )
        .unwrap();
    assert_eq!(matches.len(), 1);
    assert_eq!(matches[0].path, "docs/notes.txt");
    assert_eq!(matches[0].line_number, None);
    assert_eq!(matches[0].line, "key: value: still content");

    let no_match = cli.grep("definitely absent", "HEAD", None).unwrap_err();
    assert_eq!(no_match.code(), ErrorCode::ExternalService);
    assert_eq!(no_match.message(), "external service error (git)");
    assert!(
        no_match
            .cause()
            .expect("preserve grep no-match diagnostics")
            .to_string()
            .contains("grep")
    );

    let empty_dir = helpers::TestRepo::empty_dir();
    let outside_repo = GitCli::new(empty_dir.path().to_path_buf());
    let ignore_err = outside_repo.is_ignored("visible.txt").unwrap_err();
    assert_eq!(ignore_err.code(), ErrorCode::ExternalService);
    assert_eq!(ignore_err.message(), "external service error (git)");
    assert!(
        ignore_err
            .cause()
            .expect("preserve check-ignore diagnostics")
            .to_string()
            .contains("check-ignore")
    );
}

#[test]
fn cli_stash_clean_tree_reports_typed_git_error() {
    let repo = helpers::TestRepo::init();
    let cli = GitCli::new(repo.path().to_path_buf());

    let err = cli.stash("clean tree").unwrap_err();
    assert_eq!(err.code(), ErrorCode::ExternalService);
    assert_eq!(err.message(), "external service error (git)");
    assert!(
        err.cause()
            .expect("preserve stash lookup diagnostics")
            .to_string()
            .contains("stash@{0}")
    );
}
