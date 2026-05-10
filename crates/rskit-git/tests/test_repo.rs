mod helpers;

use rskit_git::{Repository, discover, init, init_bare, open};

use tempfile::TempDir;

#[test]
fn test_open() {
    let repo = helpers::TestRepo::init();
    let r = open(repo.path()).expect("open failed");
    assert_eq!(
        r.root().canonicalize().unwrap(),
        repo.path().canonicalize().unwrap()
    );
}

#[test]
fn test_open_nonexistent() {
    assert!(open("/nonexistent/path").is_err());
}

#[test]
fn test_init() {
    let dir = TempDir::new().expect("failed to create temp dir");
    let r = init(dir.path()).expect("init failed");
    assert_eq!(
        r.root().canonicalize().unwrap(),
        dir.path().canonicalize().unwrap()
    );
    assert!(dir.path().join(".git").is_dir());
}

#[test]
fn test_init_bare() {
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
fn test_discover() {
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
fn test_head() {
    let repo = helpers::TestRepo::init();
    let r = open(repo.path()).unwrap();
    let head = r.head().unwrap();
    assert!(!head.target.is_zero());
}

#[test]
fn test_resolve_ref() {
    let repo = helpers::TestRepo::init();
    repo.create_branch("feature");
    let r = open(repo.path()).unwrap();
    let oid = r.resolve_ref("feature").unwrap();
    assert!(!oid.is_zero());
}

#[test]
fn test_resolve_ref_not_found() {
    let repo = helpers::TestRepo::init();
    let r = open(repo.path()).unwrap();
    assert!(r.resolve_ref("nonexistent").is_err());
}

#[test]
fn test_is_dirty_clean() {
    let repo = helpers::TestRepo::init();
    let r = open(repo.path()).unwrap();
    assert!(!r.is_dirty().unwrap());
}

#[test]
fn test_is_dirty_modified() {
    let repo = helpers::TestRepo::init();
    repo.make_dirty("README.md");
    let r = open(repo.path()).unwrap();
    assert!(r.is_dirty().unwrap());
}

#[test]
fn test_is_dirty_untracked() {
    let repo = helpers::TestRepo::init();
    repo.make_untracked("new.txt");
    let r = open(repo.path()).unwrap();
    assert!(r.is_dirty().unwrap());
}
