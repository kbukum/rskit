mod helpers;

use rskit_git::{
    BranchFilter, CleanOptions, ConfigReader, Maintainer, RefManager, RemoteManager, open,
};

#[test]
fn test_list_branches_and_tags() {
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
fn test_create_and_delete_branch_and_tag() {
    let repo = helpers::TestRepo::init();
    let r = open(repo.path()).unwrap();

    r.create_branch("feature", "HEAD").unwrap();
    assert!(
        r.list_branches(BranchFilter::Local)
            .unwrap()
            .iter()
            .any(|branch| branch.name == "feature")
    );

    r.create_tag("v1.0.0", "HEAD", "release 1.0.0").unwrap();
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
fn test_list_remotes() {
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
fn test_tracking_branch() {
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
fn test_config_get() {
    let repo = helpers::TestRepo::init();
    let r = open(repo.path()).unwrap();

    assert_eq!(r.config_get("user.name").unwrap(), "Test User");
}

#[test]
fn test_config_get_all() {
    let repo = helpers::TestRepo::init();
    repo.config_add("test.multi", "one");
    repo.config_add("test.multi", "two");
    let r = open(repo.path()).unwrap();

    assert_eq!(r.config_get_all("test.multi").unwrap(), vec!["one", "two"]);
}

#[test]
fn test_config_set() {
    let repo = helpers::TestRepo::init();
    repo.config_set("test.value", "before");
    let r = open(repo.path()).unwrap();

    r.config_set("test.value", "after").unwrap();
    assert_eq!(r.config_get("test.value").unwrap(), "after");
}

#[test]
fn test_gc_and_fsck() {
    let repo = helpers::TestRepo::init();
    let r = open(repo.path()).unwrap();

    r.gc().unwrap();
    r.prune().unwrap();
    r.fsck().unwrap();
}

#[test]
fn test_clean() {
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
