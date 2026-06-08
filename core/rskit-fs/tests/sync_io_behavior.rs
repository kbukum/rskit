//! Behavioral coverage for public synchronous filesystem helpers.

use rskit_fs::TempDir;
use rskit_fs::sync_io::{dir, file};

#[test]
fn sync_file_helpers_cover_nested_create_copy_move_metadata_and_remove() {
    let root = TempDir::new().unwrap();
    let source = root.child("nested/source.txt").unwrap();
    let copied = root.child("copied/out.txt").unwrap();
    let moved = root.child("moved/final.txt").unwrap();

    file::write(&source, b"hello").unwrap();
    assert!(file::exists(&source).unwrap());
    assert_eq!(file::read(&source).unwrap(), b"hello");
    assert_eq!(file::read_string(&source).unwrap(), "hello");

    let copied_bytes = file::copy(&source, &copied).unwrap();
    assert_eq!(copied_bytes, 5);
    assert_eq!(file::read_string(&copied).unwrap(), "hello");

    file::rename(&copied, &moved).unwrap();
    assert!(!file::exists(&copied).unwrap());
    assert_eq!(file::read_string(&moved).unwrap(), "hello");

    let metadata = file::metadata(&moved).unwrap();
    assert_eq!(metadata.path, moved);
    assert_eq!(metadata.len, 5);
    assert!(metadata.is_file);
    assert!(!metadata.is_dir);
    assert!(!metadata.is_symlink);

    assert!(file::remove_if_exists(&moved).unwrap());
    assert!(!file::remove_if_exists(&moved).unwrap());
}

#[test]
fn sync_file_atomic_replace_and_move_file_create_destination_parents() {
    let root = TempDir::new().unwrap();
    let atomic = root.child("atomic/value.txt").unwrap();
    let moved = root.child("deep/moved/value.txt").unwrap();

    file::write_atomic(&atomic, b"first", "atomic").unwrap();
    assert_eq!(file::read_string(&atomic).unwrap(), "first");
    file::write_atomic(&atomic, b"second", "atomic").unwrap();
    assert_eq!(file::read_string(&atomic).unwrap(), "second");

    file::write_atomic_replace(&atomic, b"third", "atomic").unwrap();
    assert_eq!(file::read_string(&atomic).unwrap(), "third");

    file::move_file(&atomic, &moved).unwrap();
    assert!(!file::exists(&atomic).unwrap());
    assert_eq!(file::read_string(&moved).unwrap(), "third");
    assert!(file::canonicalize(&moved).unwrap().is_absolute());
}

#[test]
fn sync_file_helpers_return_typed_errors_for_missing_and_invalid_utf8_inputs() {
    let root = TempDir::new().unwrap();
    let missing = root.child("missing.txt").unwrap();
    let binary = root.write_file("binary.bin", &[0xff, 0xfe]).unwrap();

    assert!(!file::exists(&missing).unwrap());
    assert!(file::open(&missing).is_err());
    assert!(file::read(&missing).is_err());
    assert!(file::read_string(&missing).is_err());
    assert!(file::read_string_bounded(&binary, 16).is_err());
    assert!(file::metadata(&missing).is_err());
    assert!(file::canonicalize(&missing).is_err());
    assert!(!file::remove_if_exists(&missing).unwrap());
}

#[test]
fn sync_dir_helpers_cover_listing_empty_remove_and_missing_paths() {
    let root = TempDir::new().unwrap();
    let parent = root.child("parent").unwrap();
    let child_dir = root.child("parent/child").unwrap();
    let child_file = root.child("parent/file.txt").unwrap();

    dir::create_all(&child_dir).unwrap();
    file::write(&child_file, b"contents").unwrap();
    assert!(dir::exists(&parent).unwrap());
    assert!(!dir::is_empty(&parent).unwrap());

    let mut entries = dir::list(&parent).unwrap();
    entries.sort_by(|left, right| left.file_name.cmp(&right.file_name));
    assert_eq!(entries.len(), 2);
    assert!(entries.iter().any(|entry| entry.is_dir && !entry.is_file));
    assert!(entries.iter().any(|entry| entry.is_file && !entry.is_dir));
    assert!(entries.iter().all(|entry| !entry.is_symlink));

    assert!(dir::remove_all_if_exists(&parent).unwrap());
    assert!(!dir::exists(&parent).unwrap());
    assert!(!dir::remove_all_if_exists(&parent).unwrap());

    let empty = root.child("empty").unwrap();
    dir::create_all(&empty).unwrap();
    assert!(dir::is_empty(&empty).unwrap());
    dir::remove(&empty).unwrap();
    assert!(!dir::remove_if_exists(&empty).unwrap());
}
