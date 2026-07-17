mod helpers;

use rskit_git::{Repository, discover, init, init_bare, open};

use tempfile::TempDir;

#[test]
fn open_existing_repository_returns_root() {
    let repo = helpers::TestRepo::init();
    let r = open(repo.path()).expect("open failed");
    assert_eq!(
        r.root().canonicalize().unwrap(),
        repo.path().canonicalize().unwrap()
    );
}

#[test]
fn open_nonexistent_repository_returns_error() {
    assert!(open("/nonexistent/path").is_err());
}

#[test]
fn init_creates_worktree_repository() {
    let dir = TempDir::new().expect("failed to create temp dir");
    let r = init(dir.path()).expect("init failed");
    assert_eq!(
        r.root().canonicalize().unwrap(),
        dir.path().canonicalize().unwrap()
    );
    assert!(dir.path().join(".git").is_dir());
}

#[test]
fn init_bare_creates_bare_repository_layout() {
    let dir = TempDir::new().expect("failed to create temp dir");
    let r = init_bare(dir.path()).expect("init_bare failed");
    assert_eq!(
        r.root().canonicalize().unwrap(),
        dir.path().canonicalize().unwrap()
    );
    assert!(dir.path().join("HEAD").is_file());
    assert!(!dir.path().join(".git").exists());
}

#[test]
fn discover_finds_repository_from_nested_directory() {
    let repo = helpers::TestRepo::init();
    let subdir = repo.path().join("sub/deep");
    std::fs::create_dir_all(&subdir).unwrap();
    let r = discover(&subdir).expect("discover failed");
    assert_eq!(
        r.root().canonicalize().unwrap(),
        repo.path().canonicalize().unwrap()
    );
    assert_ne!(
        r.root().canonicalize().unwrap(),
        subdir.canonicalize().unwrap()
    );
}

#[test]
fn head_returns_current_commit() {
    let repo = helpers::TestRepo::init();
    let r = open(repo.path()).unwrap();
    let head = r.head().unwrap();
    assert!(!head.target.is_zero());
}

#[test]
fn resolve_ref_returns_branch_target() {
    let repo = helpers::TestRepo::init();
    repo.create_branch("feature");
    let r = open(repo.path()).unwrap();
    let oid = r.resolve_ref("feature").unwrap();
    assert!(!oid.is_zero());
}

#[test]
fn resolve_ref_returns_error_for_unknown_ref() {
    let repo = helpers::TestRepo::init();
    let r = open(repo.path()).unwrap();
    assert!(r.resolve_ref("nonexistent").is_err());
}

#[test]
fn is_dirty_is_false_for_clean_worktree() {
    let repo = helpers::TestRepo::init();
    let r = open(repo.path()).unwrap();
    assert!(!r.is_dirty().unwrap());
}

#[test]
fn is_dirty_is_true_for_modified_tracked_file() {
    let repo = helpers::TestRepo::init();
    repo.make_dirty("README.md");
    let r = open(repo.path()).unwrap();
    assert!(r.is_dirty().unwrap());
}

#[test]
fn is_dirty_is_true_for_untracked_file() {
    let repo = helpers::TestRepo::init();
    repo.make_untracked("new.txt");
    let r = open(repo.path()).unwrap();
    assert!(r.is_dirty().unwrap());
}
