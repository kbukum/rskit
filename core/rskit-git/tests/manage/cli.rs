use crate::helpers;

use std::path::Path;

use rskit_errors::ErrorCode;
use rskit_git::cli::GitCli;
use rskit_git::{
    CleanOptions, ConfigReader, Maintainer, PushOptions, RefManager, RemoteManager, Stasher, open,
};

#[test]
fn paths_map_resolution_join_and_validation_edges() {
    let workspace = helpers::TestRepo::empty_dir();
    let missing_root = workspace.path().join("missing-root");
    let err = rskit_git::repo_relative_path(&missing_root, workspace.path())
        .expect_err("missing git root should be rejected");
    assert_eq!(err.code(), ErrorCode::InvalidInput);
    assert!(
        err.message().contains("failed to resolve git root"),
        "unexpected message: {}",
        err.message()
    );

    let joined = rskit_git::join_repo_path(Path::new("repo/."), Path::new("./src/./lib.rs"))
        .expect("valid repository paths should join");
    assert_eq!(joined, Path::new("repo/src/lib.rs"));

    let rejected = rskit_git::join_repo_path(Path::new("repo"), Path::new("src/../secret"))
        .expect_err("relative path with parent traversal should be rejected");
    assert_eq!(rejected.code(), ErrorCode::InvalidInput);
    assert!(
        rejected.message().contains("repository path"),
        "unexpected message: {}",
        rejected.message()
    );
}

#[test]
fn cli_list_tags_handles_empty_output_or_cli_incompatibility() {
    let repo = helpers::TestRepo::init();
    let cli = GitCli::new(repo.path().to_path_buf());

    match cli.list_tags() {
        Ok(tags) => assert!(tags.is_empty()),
        Err(err) => {
            assert_eq!(err.code(), ErrorCode::ExternalService);
            assert!(
                err.message().contains("external service error"),
                "unexpected message: {}",
                err.message()
            );
        }
    }
}

#[test]
fn cli_delete_missing_refs_and_missing_config_are_typed_errors() {
    let repo = helpers::TestRepo::init();
    let cli = GitCli::new(repo.path().to_path_buf());

    let missing_branch = cli.delete_branch("does-not-exist").unwrap_err();
    assert_eq!(missing_branch.code(), ErrorCode::NotFound);
    assert!(missing_branch.message().contains("does-not-exist"));

    let missing_tag = cli.delete_tag("does-not-exist").unwrap_err();
    assert_eq!(missing_tag.code(), ErrorCode::NotFound);
    assert!(missing_tag.message().contains("does-not-exist"));

    let missing_config = cli.config_get("rskit.missing").unwrap_err();
    assert_eq!(missing_config.code(), ErrorCode::NotFound);
    assert!(missing_config.message().contains("rskit.missing"));
}

#[test]
fn cli_config_failures_map_unexpected_config_exit_to_command_errors() {
    let repo = helpers::TestRepo::init();
    std::fs::write(repo.path().join(".git/config"), "[broken\n").unwrap();
    let cli = GitCli::new(repo.path().to_path_buf());

    let get_error = cli.config_get("core.repositoryformatversion").unwrap_err();
    assert_eq!(get_error.code(), ErrorCode::ExternalService);
    assert!(
        get_error.message().contains("external service error"),
        "unexpected message: {}",
        get_error.message()
    );

    let get_all_error = cli.config_get_all("remote.origin.fetch").unwrap_err();
    assert_eq!(get_all_error.code(), ErrorCode::ExternalService);
    assert!(
        get_all_error.message().contains("external service error"),
        "unexpected message: {}",
        get_all_error.message()
    );
}

#[test]
fn cli_remotes_skip_malformed_lines_and_push_force_to_local_remote() {
    let repo = helpers::TestRepo::init();
    let remote = helpers::TestRepo::init_bare();
    repo.add_remote("origin", remote.path().to_str().unwrap());
    repo.config_set("remote.broken.url", "line1\nline2");
    let cli = GitCli::new(repo.path().to_path_buf());

    let remotes = cli.list_remotes().expect("remotes should list");
    let broken = remotes
        .iter()
        .find(|remote| remote.name == "broken")
        .expect("remote with multiline url should still produce first record");
    assert_eq!(broken.url, "line1");
    assert!(
        remotes.iter().all(|remote| remote.name != "line2 (fetch)"),
        "malformed continuation lines must be ignored"
    );

    let branch = repo.current_branch();
    cli.push(
        "origin",
        Some(&PushOptions {
            force: true,
            refspecs: vec![format!("refs/heads/{branch}:refs/heads/{branch}")],
            ..Default::default()
        }),
    )
    .expect("forced push to local bare repo should succeed");
}

#[test]
fn cli_clean_with_directories_reports_nested_untracked_directory() {
    let repo = helpers::TestRepo::init();
    std::fs::create_dir_all(repo.path().join("target-like")).unwrap();
    std::fs::write(repo.path().join("target-like/out.txt"), "artifact\n").unwrap();
    let cli = GitCli::new(repo.path().to_path_buf());

    let cleaned = cli
        .clean(Some(&CleanOptions {
            directories: true,
            ..Default::default()
        }))
        .expect("dry-run clean should succeed");

    assert!(cleaned.iter().any(|path| path == "target-like/"));
    assert!(repo.path().join("target-like/out.txt").exists());
}

#[test]
fn repo_stasher_delegates_pop_by_index_to_cli_backend() {
    let repo = helpers::TestRepo::init();
    let opened = open(repo.path()).expect("repo should open");
    repo.make_dirty("README.md");
    opened
        .stash("delegated stash")
        .expect("stash should be created");

    Stasher::stash_pop_index(&opened, 0).expect("stash index should pop through Repo delegation");

    let readme = std::fs::read_to_string(repo.path().join("README.md")).unwrap();
    assert_eq!(readme, "dirty content\n");
}
