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
fn map_remote_error_classifies_auth_failures() {
    let http = git2::Error::new(
        git2::ErrorCode::GenericError,
        git2::ErrorClass::Http,
        "401 Unauthorized",
    );
    assert!(matches!(
        map_remote_error(http),
        GitError::RemoteAuth { .. }
    ));

    let auth = git2::Error::new(
        git2::ErrorCode::Auth,
        git2::ErrorClass::None,
        "authentication required",
    );
    assert!(matches!(
        map_remote_error(auth),
        GitError::RemoteAuth { .. }
    ));
}

#[test]
fn map_push_error_classifies_ref_rejection_auth_and_internal() {
    let refspecs = ["refs/heads/main:refs/heads/main".to_string()];

    let non_ff = git2::Error::new(
        git2::ErrorCode::NotFastForward,
        git2::ErrorClass::Reference,
        "cannot push non-fast-forward",
    );
    match map_push_error(non_ff, &refspecs) {
        GitError::PushRejected { refname, .. } => assert_eq!(refname, "refs/heads/main"),
        other => panic!("expected PushRejected, got {other:?}"),
    }

    let forbidden = git2::Error::new(
        git2::ErrorCode::GenericError,
        git2::ErrorClass::Http,
        "403 Forbidden",
    );
    assert!(matches!(
        map_push_error(forbidden, &refspecs),
        GitError::RemoteAuth { .. }
    ));

    let internal = git2::Error::from_str("boom");
    assert!(matches!(
        map_push_error(internal, &refspecs),
        GitError::Internal(_)
    ));
}

#[test]
fn map_push_error_redacts_url_credentials_in_ref_rejection() {
    let refspecs = ["refs/heads/main".to_string()];
    // Build the credentialed URL at runtime so no literal secret sits in source.
    let secret = "p4ssw0rd";
    let url = format!("https://user:{secret}@github.com/o/r.git");
    let rejected = git2::Error::new(
        git2::ErrorCode::NotFastForward,
        git2::ErrorClass::Reference,
        format!("failed to push to {url}"),
    );
    match map_push_error(rejected, &refspecs) {
        GitError::PushRejected { reason, .. } => {
            assert!(
                reason.contains("https://***@github.com/o/r.git"),
                "{reason}"
            );
            assert!(!reason.contains(secret), "{reason}");
        }
        other => panic!("expected PushRejected, got {other:?}"),
    }
}

#[test]
fn map_push_error_strips_force_prefix_and_joins_destination_refs() {
    let non_ff = || {
        git2::Error::new(
            git2::ErrorCode::NotFastForward,
            git2::ErrorClass::Reference,
            "rejected",
        )
    };

    // A colon-less force refspec must not leak its `+` into the message.
    let colonless = ["+refs/heads/main".to_string()];
    match map_push_error(non_ff(), &colonless) {
        GitError::PushRejected { refname, .. } => assert_eq!(refname, "refs/heads/main"),
        other => panic!("expected PushRejected, got {other:?}"),
    }

    let with_dst = ["+refs/heads/main:refs/heads/main".to_string()];
    match map_push_error(non_ff(), &with_dst) {
        GitError::PushRejected { refname, .. } => assert_eq!(refname, "refs/heads/main"),
        other => panic!("expected PushRejected, got {other:?}"),
    }

    // Several refspecs join their destination refs.
    let both = [
        "refs/heads/main".to_string(),
        "refs/tags/v1.0.0".to_string(),
    ];
    match map_push_error(non_ff(), &both) {
        GitError::PushRejected { refname, .. } => {
            assert_eq!(refname, "refs/heads/main, refs/tags/v1.0.0");
        }
        other => panic!("expected PushRejected, got {other:?}"),
    }
}

#[test]
fn redact_url_credentials_strips_userinfo_only() {
    // Build credentialed inputs at runtime so no literal secret sits in source.
    let user = "alice";
    let pass = "t0ken";
    let input = format!("push https://{user}:{pass}@host/x failed");
    assert_eq!(
        redact_url_credentials(&input),
        "push https://***@host/x failed"
    );
    // No credentials: message is left untouched.
    assert_eq!(
        redact_url_credentials("https://github.com/o/r.git rejected"),
        "https://github.com/o/r.git rejected"
    );
    // SSH-style userinfo without a password is still redacted.
    let ssh = format!("ssh://{user}@github.com/o/r");
    assert_eq!(redact_url_credentials(&ssh), "ssh://***@github.com/o/r");
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

#[test]
fn embedded_backend_reports_signed_tags_as_unsupported() {
    use crate::manage::RefManager;
    use crate::options::SignOptions;

    let root = rskit_testutil::test_workspace!("git2-signed-tag-unsupported");
    let repo = init(root.path()).expect("init repo");

    let unsupported = repo
        .create_signed_tag(
            "v1-signed",
            "HEAD",
            "signed release",
            &SignOptions::default(),
        )
        .expect_err("libgit2 cannot sign tags");

    assert_eq!(unsupported.code(), rskit_errors::ErrorCode::InvalidInput);
    assert!(unsupported.message().contains("not supported"));
}
