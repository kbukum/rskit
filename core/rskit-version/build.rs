//! Build script that captures git and build metadata as compile-time environment variables.
//!
//! Build/version values are emitted via `cargo:rustc-env` and read back in `lib.rs` with `env!`.
//! The build script intentionally avoids extra dependencies and external commands beyond `git`/`rustc`;
//! the build timestamp is emitted as a Unix epoch (`BUILD_EPOCH`)
//! and formatted to RFC 3339 at runtime by the library, which keeps the conversion testable
//! and portable across platforms. `SOURCE_DATE_EPOCH` overrides the timestamp for reproducible
//! builds.

use std::env;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::{SystemTime, UNIX_EPOCH};

/// Largest Unix epoch second the runtime RFC 3339 formatter can represent: `9999-12-31T23:59:59Z`.
///
/// The library parses `BUILD_EPOCH` as `i64` and formats it with a four-digit RFC 3339 year, so any
/// larger `SOURCE_DATE_EPOCH` (including values above `i64::MAX`) would format to an empty
/// timestamp. Rejecting such values here fails the build at the boundary instead.
const MAX_REPRESENTABLE_EPOCH: i64 = 253_402_300_799;

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

/// Seconds since the Unix epoch at build time, honoring `SOURCE_DATE_EPOCH` for reproducible
/// builds, or empty if the clock is before the epoch (which should never happen in practice).
///
/// When `SOURCE_DATE_EPOCH` is set it must be a non-negative integer within the range the runtime
/// RFC 3339 formatter can represent (up to `9999-12-31T23:59:59Z`, i.e. [`MAX_REPRESENTABLE_EPOCH`]);
/// a present-but-malformed or out-of-range value fails the build rather than silently falling back
/// to the wall clock or emitting a value the library would format to an empty `build_time`/
/// `build_date`, either of which would make a build that explicitly requested a reproducible
/// timestamp nondeterministic. The wall clock is used only when the variable is absent.
fn build_epoch() -> String {
    match env::var("SOURCE_DATE_EPOCH") {
        Ok(raw) => {
            let trimmed = raw.trim();
            if !trimmed.is_empty() && trimmed.bytes().all(|b| b.is_ascii_digit()) {
                match trimmed.parse::<i64>() {
                    Ok(secs) if secs <= MAX_REPRESENTABLE_EPOCH => return trimmed.to_owned(),
                    _ => {}
                }
            }
            panic!(
                "SOURCE_DATE_EPOCH is set to {raw:?}, which is not a non-negative integer within \
                 the representable range [0, {MAX_REPRESENTABLE_EPOCH}] (up to 9999-12-31T23:59:59Z); \
                 a reproducible build must not fall back to the wall clock or emit an empty build timestamp"
            );
        }
        Err(env::VarError::NotPresent) => SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_secs().to_string())
            .unwrap_or_default(),
        Err(env::VarError::NotUnicode(_)) => panic!(
            "SOURCE_DATE_EPOCH is set to a non-Unicode value; \
             a reproducible build must not fall back to the wall clock"
        ),
    }
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

/// Track the git metadata files that determine the captured commit, branch, and dirty state.
///
/// The git directory is resolved via git itself (`--absolute-git-dir`) because the build script
/// runs from the crate directory, while `.git` typically lives at the workspace root. `HEAD` and
/// the resolved ref cover commit/branch changes (a branch commit updates a ref, not `HEAD`);
/// `index` covers staged changes, which is what most `git add`/commit workflows touch. Tracking
/// stable inputs keeps the script cacheable so it does not invalidate `rskit-version` and its
/// reverse dependencies on every build. Best-effort: anything missing is simply skipped.
///
/// Purely unstaged edits to tracked files do not touch any of these inputs, so `GIT_DIRTY` may lag
/// until the index or a ref moves; release metadata should be injected via `SOURCE_DATE_EPOCH` (and
/// a clean checkout) rather than inferred from an incremental developer build.
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
    rerun_if_exists(&git_dir.join("index"));
    rerun_if_exists(&common_git_dir.join("index"));
}

fn main() {
    let git_commit = run_git(&["rev-parse", "HEAD"]);
    let git_dirty = if run_git(&["status", "--porcelain"]).is_empty() {
        "false"
    } else {
        "true"
    };

    // `--abbrev-ref HEAD` yields the literal "HEAD" in detached-HEAD state (e.g. CI checkouts);
    // normalize that to "no branch".
    let mut git_branch = run_git(&["rev-parse", "--abbrev-ref", "HEAD"]);
    if git_branch == "HEAD" {
        git_branch.clear();
    }

    let build_epoch = build_epoch();
    let rust_version = rust_version();

    println!("cargo:rustc-env=GIT_COMMIT={git_commit}");
    println!("cargo:rustc-env=GIT_BRANCH={git_branch}");
    println!("cargo:rustc-env=GIT_DIRTY={git_dirty}");
    println!("cargo:rustc-env=BUILD_EPOCH={build_epoch}");
    println!("cargo:rustc-env=RUST_VERSION_STR={rust_version}");

    println!("cargo:rerun-if-env-changed=SOURCE_DATE_EPOCH");
    track_git_inputs();
}
