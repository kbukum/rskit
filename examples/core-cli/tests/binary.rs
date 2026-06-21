//! Binary-level tests for `core-cli`.
//!
//! These spawn the compiled `core-cli` binary (via `CARGO_BIN_EXE_core-cli`,
//! the same pattern `agent-demo` uses) so the `main` entry point, error
//! rendering, and exit-code mapping are exercised end to end — including under
//! coverage.

use std::path::{Path, PathBuf};
use std::process::Command;

/// Path to the compiled `core-cli` binary, provided by Cargo at build time.
fn binary() -> &'static str {
    env!("CARGO_BIN_EXE_core-cli")
}

/// Portable path to a fixture under `fixtures/`.
fn fixture(name: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("fixtures")
        .join(name)
}

#[test]
fn version_command_succeeds() {
    let output = Command::new(binary())
        .arg("version")
        .output()
        .expect("spawn core-cli version");

    assert!(output.status.success(), "version should exit 0");
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("core-cli"));
    assert!(stdout.contains("rskit-version"));
}

#[test]
fn show_command_renders_settings() {
    let output = Command::new(binary())
        .arg("show")
        .arg(fixture("app.toml"))
        .output()
        .expect("spawn core-cli show");

    assert!(output.status.success(), "show should exit 0");
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("core-cli-demo"));
}

#[test]
fn run_command_processes_units() {
    let output = Command::new(binary())
        .args(["run", "2"])
        .output()
        .expect("spawn core-cli run");

    assert!(output.status.success(), "run should exit 0");
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("processed"));
}

#[test]
fn unknown_command_exits_with_usage_code() {
    let output = Command::new(binary())
        .arg("bogus")
        .output()
        .expect("spawn core-cli bogus");

    // ExitCode::Usage == 2 for invalid command input.
    assert_eq!(output.status.code(), Some(2));
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("unknown command"));
}

#[test]
fn show_without_path_exits_with_usage_code() {
    let output = Command::new(binary())
        .arg("show")
        .output()
        .expect("spawn core-cli show (no path)");

    assert_eq!(output.status.code(), Some(2));
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("missing"));
}
