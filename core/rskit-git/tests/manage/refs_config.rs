use crate::helpers;

use rskit_git::{
    BranchFilter, CleanOptions, ConfigReader, FetchOptions, Maintainer, PushOptions, RefManager,
    RemoteManager, Repository, cli::GitCli, open,
};

#[test]
fn list_branches_and_tags_returns_local_refs() {
    let repo = helpers::TestRepo::init();
    repo.create_branch("feature");
    repo.create_tag("v0.1.0");
    repo.create_annotated_tag("v0.2.0", "release");
    let r = open(repo.path()).unwrap();

    let branches = r.list_branches(BranchFilter::Local).unwrap();
    assert!(branches.iter().any(|branch| branch.name == "feature"));
    assert!(
        branches
            .iter()
            .any(|branch| branch.name == repo.current_branch())
    );

    let tags = r.list_tags().unwrap();
    assert!(
        tags.iter()
            .any(|tag| tag.name == "v0.1.0" && tag.message.is_empty())
    );
    assert!(
        tags.iter()
            .any(|tag| tag.name == "v0.2.0" && tag.message == "release" && tag.tagger.is_some())
    );
}

#[test]
fn create_and_delete_branch_and_tag_updates_refs() {
    let repo = helpers::TestRepo::init();
    let r = open(repo.path()).unwrap();

    r.create_branch("feature", "HEAD").unwrap();
    assert!(
        r.list_branches(BranchFilter::Local)
            .unwrap()
            .iter()
            .any(|branch| branch.name == "feature")
    );

    r.create_tag("v1.0.0", "HEAD", Some("release 1.0.0"))
        .unwrap();
    assert!(
        r.list_tags()
            .unwrap()
            .iter()
            .any(|tag| tag.name == "v1.0.0" && tag.message == "release 1.0.0")
    );

    r.delete_branch("feature").unwrap();
    assert!(
        !r.list_branches(BranchFilter::Local)
            .unwrap()
            .iter()
            .any(|branch| branch.name == "feature")
    );

    r.delete_tag("v1.0.0").unwrap();
    assert!(
        !r.list_tags()
            .unwrap()
            .iter()
            .any(|tag| tag.name == "v1.0.0")
    );
}

#[test]
fn create_tag_distinguishes_annotated_empty_message_from_lightweight() {
    let repo = helpers::TestRepo::init();
    let r = open(repo.path()).unwrap();

    // `Some("")` is the capability the old `&str` API could not express: an
    // annotated tag (tagger present) whose message happens to be empty.
    r.create_tag("annotated-empty", "HEAD", Some("")).unwrap();
    // `None` is a lightweight tag: a plain ref with no tagger.
    r.create_tag("lightweight", "HEAD", None).unwrap();

    let tags = r.list_tags().unwrap();
    let annotated = tags
        .iter()
        .find(|tag| tag.name == "annotated-empty")
        .expect("annotated tag present");
    let lightweight = tags
        .iter()
        .find(|tag| tag.name == "lightweight")
        .expect("lightweight tag present");

    assert!(
        annotated.tagger.is_some(),
        "Some(\"\") must create an annotated tag, not a lightweight one"
    );
    assert!(annotated.message.is_empty());
    assert!(
        lightweight.tagger.is_none(),
        "None must create a lightweight tag"
    );
}

#[test]
fn list_remotes_returns_fetch_specs() {
    let repo = helpers::TestRepo::init();
    let remote = helpers::TestRepo::init_bare();
    repo.add_remote("origin", remote.path().to_str().unwrap());
    let r = open(repo.path()).unwrap();

    let remotes = r.list_remotes().unwrap();
    assert_eq!(remotes.len(), 1);
    assert_eq!(remotes[0].name, "origin");
    assert_eq!(remotes[0].url, remote.path().to_str().unwrap());
    assert!(
        remotes[0]
            .fetch_specs
            .iter()
            .any(|spec| spec == "+refs/heads/*:refs/remotes/origin/*")
    );
}

#[test]
fn tracking_branch_returns_upstream_ref() {
    let repo = helpers::TestRepo::init();
    let remote = helpers::TestRepo::init_bare();
    let branch = repo.current_branch();
    repo.add_remote("origin", remote.path().to_str().unwrap());
    repo.push_upstream("origin", &branch);
    let r = open(repo.path()).unwrap();

    assert_eq!(
        r.tracking_branch(&branch).unwrap(),
        format!("origin/{branch}")
    );
}

#[test]
fn config_get_returns_value() {
    let repo = helpers::TestRepo::init();
    let r = open(repo.path()).unwrap();

    assert_eq!(r.config_get("user.name").unwrap(), "Test User");
}

#[test]
fn config_get_all_returns_multi_value_entries() {
    let repo = helpers::TestRepo::init();
    repo.config_add("test.multi", "one");
    repo.config_add("test.multi", "two");
    let r = open(repo.path()).unwrap();

    assert_eq!(r.config_get_all("test.multi").unwrap(), vec!["one", "two"]);
}

#[test]
fn config_set_replaces_value() {
    let repo = helpers::TestRepo::init();
    repo.config_set("test.value", "before");
    let r = open(repo.path()).unwrap();

    r.config_set("test.value", "after").unwrap();
    assert_eq!(r.config_get("test.value").unwrap(), "after");
}

#[test]
fn maintenance_commands_cover_gc_and_complete_stable_operations() {
    let repo = helpers::TestRepo::init();
    let r = open(repo.path()).unwrap();

    let _ = r.gc();
    r.prune().unwrap();
    r.fsck().unwrap();
}

#[test]
fn clean_removes_forced_untracked_files_and_directories() {
    let repo = helpers::TestRepo::init();
    std::fs::write(repo.path().join("scratch.txt"), "scratch\n").unwrap();
    std::fs::create_dir_all(repo.path().join("build")).unwrap();
    std::fs::write(repo.path().join("build/out.txt"), "out\n").unwrap();
    let r = open(repo.path()).unwrap();

    let cleaned = r
        .clean(Some(&CleanOptions {
            directories: true,
            force: true,
            ..Default::default()
        }))
        .unwrap();

    assert!(cleaned.iter().any(|path| path == "scratch.txt"));
    assert!(cleaned.iter().any(|path| path == "build/"));
    assert!(!repo.path().join("scratch.txt").exists());
    assert!(!repo.path().join("build").exists());
}

#[test]
fn missing_refs_and_config_map_to_typed_errors() {
    let repo = helpers::TestRepo::init();
    let r = open(repo.path()).unwrap();

    let missing_branch = r.delete_branch("does-not-exist").unwrap_err();
    assert_eq!(missing_branch.code(), rskit_errors::ErrorCode::NotFound);

    let missing_tag = r.delete_tag("does-not-exist").unwrap_err();
    assert_eq!(missing_tag.code(), rskit_errors::ErrorCode::NotFound);

    let missing_config = r.config_get("rskit.missing").unwrap_err();
    assert_eq!(missing_config.code(), rskit_errors::ErrorCode::NotFound);
}

#[test]
fn local_fetch_and_push_respect_explicit_refspec_options() {
    let source = helpers::TestRepo::init();
    let remote = helpers::TestRepo::init_bare();
    let clone_dir = helpers::TestRepo::empty_dir();
    let source_branch = source.current_branch();
    source.add_remote("origin", remote.path().to_str().unwrap());
    let source_repo = open(source.path()).unwrap();

    source_repo
        .push(
            "origin",
            Some(&PushOptions {
                refspecs: vec![format!(
                    "refs/heads/{source_branch}:refs/heads/{source_branch}"
                )],
                ..Default::default()
            }),
        )
        .expect("push to local bare repo");

    let clone = rskit_git::clone(remote.path().to_str().unwrap(), clone_dir.path())
        .expect("clone local bare repo");
    source.commit_file("after-clone.txt", "new\n", "new work");
    source_repo
        .push(
            "origin",
            Some(&PushOptions {
                force: true,
                refspecs: vec![format!(
                    "refs/heads/{source_branch}:refs/heads/{source_branch}"
                )],
                ..Default::default()
            }),
        )
        .expect("push update to local bare repo");

    clone
        .fetch(
            "origin",
            Some(&FetchOptions {
                prune: true,
                refspecs: vec![format!(
                    "refs/heads/{source_branch}:refs/remotes/origin/{source_branch}"
                )],
                ..Default::default()
            }),
        )
        .expect("fetch update from local bare repo");

    assert!(
        clone
            .resolve_ref(&format!("origin/{source_branch}"))
            .is_ok()
    );
}

#[test]
fn clean_dry_run_reports_ignored_paths_without_removing_them() {
    let repo = helpers::TestRepo::init();
    std::fs::write(repo.path().join(".gitignore"), "*.log\n").unwrap();
    std::fs::write(repo.path().join("debug.log"), "debug\n").unwrap();
    let r = open(repo.path()).unwrap();

    let cleaned = r
        .clean(Some(&CleanOptions {
            ignored: true,
            ..Default::default()
        }))
        .unwrap();

    assert!(cleaned.iter().any(|path| path == "debug.log"));
    assert!(repo.path().join("debug.log").exists());
}

#[test]
fn cli_backend_covers_local_ref_remote_config_and_clean_paths() {
    let repo = helpers::TestRepo::init();
    let remote = helpers::TestRepo::init_bare();
    let cli = GitCli::new(repo.path().to_path_buf());

    cli.create_branch("cli-feature", "HEAD").unwrap();
    assert!(
        cli.list_branches(BranchFilter::All)
            .unwrap()
            .iter()
            .any(|branch| branch.name == "cli-feature")
    );
    assert!(
        cli.list_branches(BranchFilter::Local)
            .unwrap()
            .iter()
            .any(|branch| branch.name == "cli-feature")
    );
    assert!(cli.list_branches(BranchFilter::Remote).unwrap().is_empty());

    cli.create_tag("cli-lightweight", "HEAD", None).unwrap();
    cli.create_tag("cli-annotated", "HEAD", Some("annotated"))
        .unwrap();

    cli.config_set("test.cli", "one").unwrap();
    repo.config_add("test.cli.multi", "a");
    repo.config_add("test.cli.multi", "b");
    assert_eq!(cli.config_get("test.cli").unwrap(), "one");
    assert_eq!(
        cli.config_get_all("test.cli.multi").unwrap(),
        vec!["a", "b"]
    );
    assert!(cli.config_get_all("test.cli.missing").unwrap().is_empty());

    repo.add_remote("origin", remote.path().to_str().unwrap());
    let remotes = cli.list_remotes().unwrap();
    assert_eq!(remotes[0].name, "origin");
    assert!(
        remotes[0]
            .fetch_specs
            .iter()
            .any(|spec| spec.contains("origin"))
    );
    let branch = repo.current_branch();
    cli.push(
        "origin",
        Some(&PushOptions {
            extra_args: vec!["-u".to_string()],
            refspecs: vec![format!("refs/heads/{branch}:refs/heads/{branch}")],
            ..Default::default()
        }),
    )
    .unwrap();
    cli.fetch(
        "origin",
        Some(&FetchOptions {
            prune: true,
            depth: Some(1),
            refspecs: vec![format!("refs/heads/{branch}:refs/remotes/origin/{branch}")],
            ..Default::default()
        }),
    )
    .unwrap();
    assert_eq!(
        cli.tracking_branch(&branch).unwrap(),
        format!("origin/{branch}")
    );

    std::fs::write(repo.path().join("scratch.tmp"), "scratch\n").unwrap();
    let dry_run = cli.clean(None).unwrap();
    assert!(dry_run.iter().any(|path| path == "scratch.tmp"));
    assert!(repo.path().join("scratch.tmp").exists());

    cli.delete_branch("cli-feature").unwrap();
    cli.delete_tag("cli-lightweight").unwrap();
    cli.delete_tag("cli-annotated").unwrap();
}
