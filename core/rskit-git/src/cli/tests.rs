use std::time::Duration;

use rskit_process::ProcessResult;

use super::*;
use crate::core::Executor;

#[test]
fn new_exposes_root_and_executes_git_commands() {
    let root = rskit_testutil::test_workspace!("git-cli-root");
    let cli = GitCli::new(root.path().to_path_buf());

    assert_eq!(cli.root(), root.path());
    let version = cli.exec(&["--version"]).expect("git version runs");
    assert!(String::from_utf8_lossy(&version).contains("git version"));
}

#[test]
fn command_failed_preserves_diagnostics_and_truncation_flags() {
    let output = ProcessResult::completed(
        Some(129),
        b"usage
"
        .to_vec(),
        b"bad args
"
        .to_vec(),
        false,
        false,
        Duration::ZERO,
        true,
        false,
    );

    let err = GitCli::command_failed(&["bad"], output);

    assert_eq!(err.code(), rskit_errors::ErrorCode::ExternalService);
    assert!(err.message().contains("external service error"));
}

#[test]
fn parse_oid_rejects_wrong_length_and_invalid_hex() {
    assert!(parse_oid("abc").is_err());
    assert!(parse_oid("zzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzz").is_err());
    assert_eq!(
        parse_oid("0000000000000000000000000000000000000001")
            .unwrap()
            .to_string(),
        "0000000000000000000000000000000000000001"
    );
}

#[test]
fn not_implemented_maps_to_internal_error() {
    let root = rskit_testutil::test_workspace!("git-cli-not-implemented");
    let cli = GitCli::new(root.path().to_path_buf());

    let err = cli
        .not_implemented::<()>("future operation")
        .expect_err("operation is not implemented");

    assert_eq!(err.code(), rskit_errors::ErrorCode::InvalidInput);
}
