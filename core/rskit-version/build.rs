//! Build script that captures git and build metadata as compile-time environment variables.

use std::process::Command;

fn run_git(args: &[&str]) -> String {
    Command::new("git")
        .args(args)
        .output()
        .ok()
        .filter(|o| o.status.success())
        .and_then(|o| String::from_utf8(o.stdout).ok())
        .map(|s| s.trim().to_owned())
        .unwrap_or_default()
}

fn build_time_rfc3339() -> String {
    // Use `date` command for a portable UTC timestamp without pulling in extra crates.
    Command::new("date")
        .args(["-u", "+%Y-%m-%dT%H:%M:%SZ"])
        .output()
        .ok()
        .filter(|o| o.status.success())
        .and_then(|o| String::from_utf8(o.stdout).ok())
        .map(|s| s.trim().to_owned())
        .unwrap_or_default()
}

fn main() {
    let git_commit = run_git(&["rev-parse", "HEAD"]);
    let git_branch = run_git(&["rev-parse", "--abbrev-ref", "HEAD"]);
    let build_time = build_time_rfc3339();

    let rust_version = Command::new("rustc")
        .arg("--version")
        .output()
        .ok()
        .filter(|o| o.status.success())
        .and_then(|o| String::from_utf8(o.stdout).ok())
        .map(|s| s.trim().to_owned())
        .unwrap_or_default();

    println!("cargo:rustc-env=GIT_COMMIT={git_commit}");
    println!("cargo:rustc-env=GIT_BRANCH={git_branch}");
    println!("cargo:rustc-env=BUILD_TIME={build_time}");
    println!("cargo:rustc-env=RUST_VERSION_STR={rust_version}");

    // Re-run when git HEAD changes
    println!("cargo:rerun-if-changed=.git/HEAD");
}
