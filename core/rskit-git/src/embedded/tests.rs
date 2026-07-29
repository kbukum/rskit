use std::time::{Duration, UNIX_EPOCH};

use crate::core::Repository;
use crate::error::GitError;

use super::conversions::system_time_from_git2;
use super::*;

#[test]
fn head_on_unborn_repository_maps_to_ref_not_found() {
    let root = rskit_testutil::test_workspace!("git2-unborn-head");
    let repo = init(root.path()).expect("init repo");

    let err = repo.head().expect_err("HEAD is unborn");

    assert_eq!(err.code(), rskit_errors::ErrorCode::NotFound);
}

#[test]
fn system_time_from_git2_handles_negative_offsets() {
    let before_epoch = system_time_from_git2(git2::Time::new(-5, 0));

    assert_eq!(before_epoch, UNIX_EPOCH - Duration::from_secs(5));
}

#[test]
fn map_remote_error_distinguishes_network_from_internal_errors() {
    let net = git2::Error::new(git2::ErrorCode::GenericError, git2::ErrorClass::Net, "down");
    assert!(matches!(map_remote_error(net), GitError::Network(_)));

    let other = git2::Error::from_str("other");
    assert!(matches!(map_remote_error(other), GitError::Internal(_)));
}

#[test]
fn map_signature_error_distinguishes_missing_identity_from_internal_errors() {
    let missing = git2::Error::new(
        git2::ErrorCode::NotFound,
        git2::ErrorClass::Config,
        "config value 'user.email' was not found",
    );
    match map_signature_error(missing) {
        GitError::IdentityMissing { key } => assert_eq!(key, "user.email"),
        other => panic!("expected IdentityMissing, got {other:?}"),
    }

    let other = git2::Error::from_str("boom");
    assert!(matches!(map_signature_error(other), GitError::Internal(_)));
}
