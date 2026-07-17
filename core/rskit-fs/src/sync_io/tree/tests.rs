use rskit_errors::ErrorCode;

use super::{canonical_dir, ensure_directory, enter_directory, metadata_for};
use crate::TempDir;

#[test]
fn ensure_directory_rejects_files() {
    let dir = TempDir::new().unwrap();
    let file = dir.write_file("file.txt", b"hello").unwrap();
    let err = ensure_directory(&file, false).unwrap_err();

    assert_eq!(err.code(), ErrorCode::InvalidInput);
}

#[cfg(unix)]
#[test]
fn ensure_directory_rejects_symlink_roots_unless_following() {
    let dir = TempDir::new().unwrap();
    let target = dir.child("target").unwrap();
    std::fs::create_dir_all(&target).unwrap();
    let link = dir.child("link").unwrap();
    std::os::unix::fs::symlink(&target, &link).unwrap();

    let err = ensure_directory(&link, false).unwrap_err();

    assert_eq!(err.code(), ErrorCode::InvalidInput);
    ensure_directory(&link, true).unwrap();
}

#[test]
fn metadata_for_reports_missing_paths() {
    let dir = TempDir::new().unwrap();
    let missing = dir.child("missing.txt").unwrap();

    assert!(metadata_for(&missing, false).is_err());
}

#[cfg(unix)]
#[test]
fn metadata_for_reports_broken_symlink_when_following() {
    let dir = TempDir::new().unwrap();
    let missing = dir.child("missing.txt").unwrap();
    let link = dir.child("link.txt").unwrap();
    std::os::unix::fs::symlink(&missing, &link).unwrap();

    assert!(metadata_for(&link, true).is_err());
}

#[test]
fn canonical_dir_reports_missing_paths() {
    let dir = TempDir::new().unwrap();
    let missing = dir.child("missing").unwrap();

    assert!(canonical_dir(&missing).is_err());
}

#[test]
fn enter_directory_rejects_cycles() {
    let dir = TempDir::new().unwrap();
    let mut visited = Some(std::collections::HashSet::new());

    enter_directory(dir.path(), &mut visited).unwrap();
    let err = enter_directory(dir.path(), &mut visited).unwrap_err();

    assert_eq!(err.code(), ErrorCode::InvalidInput);
}

#[test]
fn enter_directory_skips_tracking_when_disabled() {
    let dir = TempDir::new().unwrap();
    let mut visited = None;

    enter_directory(dir.path(), &mut visited).unwrap();

    assert!(visited.is_none());
}
