//! Build script that captures git and build metadata as compile-time
//! environment variables.
//!
//! Build/version values are emitted via `cargo:rustc-env` and read back in
//! `lib.rs` with `env!`. The build script intentionally avoids extra
//! dependencies and external commands beyond `git`/`rustc`; the build
//! timestamp is emitted as a Unix epoch (`BUILD_EPOCH`) and formatted to
//! RFC 3339 at runtime by the library, which keeps the conversion testable and
//! portable across platforms.

use std::env;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::{SystemTime, UNIX_EPOCH};

fn command_stdout(program: &str, args: &[&str]) -> String {
    Command::new(program)
        .args(args)
        .output()
        .ok()
        .filter(|o| o.status.success())
        .and_then(|o| String::from_utf8(o.stdout).ok())
        .map(|s| s.trim().to_owned())
        .unwrap_or_default()
}

fn run_git(args: &[&str]) -> String {
    command_stdout("git", args)
}

/// Seconds since the Unix epoch at build time, or empty if the clock is
/// before the epoch (which should never happen in practice).
fn build_epoch() -> String {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs().to_string())
        .unwrap_or_default()
}

fn rust_version() -> String {
    let rustc = env::var("RUSTC").unwrap_or_else(|_| "rustc".to_owned());
    let version = command_stdout(&rustc, &["--version"]);
    if version.starts_with("rustc ") {
        return version;
    }

    let rustup_rustc = command_stdout("rustup", &["which", "rustc"]);
    if !rustup_rustc.is_empty() {
        let version = command_stdout(&rustup_rustc, &["--version"]);
        if version.starts_with("rustc ") {
            return version;
        }
    }

    version
}

fn rerun_if_exists(path: &Path) {
    if path.exists() {
        println!("cargo:rerun-if-changed={}", path.display());
    }
}

/// Watch the files that determine the resolved commit so a branch commit
/// (which updates a ref, not `HEAD`) still refreshes the captured metadata.
///
/// The git directory is resolved via git itself (`--absolute-git-dir`) because
/// the build script runs from the crate directory, while `.git` typically lives
/// at the workspace root. Best-effort: anything missing is simply skipped.
fn track_git_inputs() {
    let git_dir = run_git(&["rev-parse", "--absolute-git-dir"]);
    if git_dir.is_empty() {
        return;
    }
    let git_dir = PathBuf::from(git_dir);
    let common_git_dir = run_git(&["rev-parse", "--path-format=absolute", "--git-common-dir"]);
    let common_git_dir = if common_git_dir.is_empty() {
        git_dir.clone()
    } else {
        PathBuf::from(common_git_dir)
    };

    let head = git_dir.join("HEAD");
    rerun_if_exists(&head);

    if let Ok(contents) = std::fs::read_to_string(&head)
        && let Some(reference) = contents.strip_prefix("ref:")
    {
        let reference = reference.trim();
        rerun_if_exists(&git_dir.join(reference));
        rerun_if_exists(&common_git_dir.join(reference));
    }

    rerun_if_exists(&git_dir.join("packed-refs"));
    rerun_if_exists(&common_git_dir.join("packed-refs"));
}

fn main() {
    let git_commit = run_git(&["rev-parse", "HEAD"]);

    // `--abbrev-ref HEAD` yields the literal "HEAD" in detached-HEAD state
    // (e.g. CI checkouts); normalize that to "no branch".
    let mut git_branch = run_git(&["rev-parse", "--abbrev-ref", "HEAD"]);
    if git_branch == "HEAD" {
        git_branch.clear();
    }

    let build_epoch = build_epoch();
    let rust_version = rust_version();

    println!("cargo:rustc-env=GIT_COMMIT={git_commit}");
    println!("cargo:rustc-env=GIT_BRANCH={git_branch}");
    println!("cargo:rustc-env=BUILD_EPOCH={build_epoch}");
    println!("cargo:rustc-env=RUST_VERSION_STR={rust_version}");

    track_git_inputs();
}
