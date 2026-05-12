//! Build-time version and git metadata for rskit.
//!
//! Version, git commit, branch, and build time are captured at compile time
//! via a `build.rs` script that runs `git` commands and emits `cargo:rustc-env`
//! variables.
//!
//! # Quick Start
//!
//! ```rust
//! use rskit_version::{get_version_info, get_short_version, get_full_version, is_release};
//!
//! let info = get_version_info();
//! println!("{}", info.version);      // e.g. "0.1.0"
//! println!("{}", info.git_commit);   // e.g. "a1b2c3d..."
//! println!("{}", get_short_version()); // "0.1.0-a1b2c3d"
//! println!("{}", get_full_version()); // "0.1.0-a1b2c3d (built 2024-01-15T10:30:00Z)"
//! ```

#![warn(missing_docs)]

use serde::{Deserialize, Serialize};
use std::fmt;

/// Immutable snapshot of build/version metadata. Compatible with gokit and pykit `VersionInfo`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct VersionInfo {
    /// Crate version from `Cargo.toml` (e.g. `"0.1.0"`).
    pub version: String,
    /// Full git commit hash at build time, or empty if unavailable.
    pub git_commit: String,
    /// Git branch name at build time, or empty if unavailable.
    pub git_branch: String,
    /// UTC build timestamp in RFC 3339 format, or empty if unavailable.
    pub build_time: String,
    /// Rust compiler version string (e.g. `"rustc 1.91.0 ..."`).
    pub rust_version: String,
    /// `true` when `version` is not `"dev"` and does not contain `"dirty"`.
    pub is_release: bool,
}

impl fmt::Display for VersionInfo {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", get_full_version())
    }
}

/// Returns comprehensive version information collected at compile time.
pub fn get_version_info() -> VersionInfo {
    let version = env!("CARGO_PKG_VERSION").to_owned();
    let git_commit = env!("GIT_COMMIT").to_owned();
    let git_branch = env!("GIT_BRANCH").to_owned();
    let build_time = env!("BUILD_TIME").to_owned();
    let rust_version = env!("RUST_VERSION_STR").to_owned();

    let is_release = version != "dev" && !version.contains("dirty");

    VersionInfo {
        version,
        git_commit,
        git_branch,
        build_time,
        rust_version,
        is_release,
    }
}

/// Returns a concise version string: `{version}[-{short_commit}]`.
pub fn get_short_version() -> String {
    let info = get_version_info();
    if info.git_commit.is_empty() {
        return info.version;
    }
    let short = if info.git_commit.len() > 7 {
        &info.git_commit[..7]
    } else {
        &info.git_commit
    };
    format!("{}-{short}", info.version)
}

/// Returns a detailed version string with optional branch and build time.
///
/// Format: `{version}[-{short_commit}][-{branch}] (built {time})`
/// Branches named `main` or `master` are omitted.
pub fn get_full_version() -> String {
    let info = get_version_info();
    let mut parts = vec![info.version.clone()];

    if !info.git_commit.is_empty() {
        let short = if info.git_commit.len() > 7 {
            &info.git_commit[..7]
        } else {
            &info.git_commit
        };
        parts.push(short.to_owned());
    }

    if !info.git_branch.is_empty() && info.git_branch != "main" && info.git_branch != "master" {
        parts.push(info.git_branch.clone());
    }

    let mut version = parts.join("-");

    if !info.build_time.is_empty() {
        use std::fmt::Write;
        let _ = write!(version, " (built {})", info.build_time);
    }

    version
}

/// Returns `true` when this build represents a release (not dev, not dirty).
pub fn is_release() -> bool {
    get_version_info().is_release
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn version_info_has_version() {
        let info = get_version_info();
        assert!(!info.version.is_empty(), "version must not be empty");
    }

    #[test]
    fn version_info_has_rust_version() {
        let info = get_version_info();
        assert!(
            info.rust_version.contains("rustc"),
            "rust_version should contain 'rustc', got: {}",
            info.rust_version
        );
    }

    #[test]
    fn version_info_has_build_time() {
        let info = get_version_info();
        assert!(
            info.build_time.contains('T'),
            "build_time should be RFC 3339, got: {}",
            info.build_time
        );
    }

    #[test]
    fn short_version_contains_version() {
        let sv = get_short_version();
        let info = get_version_info();
        assert!(
            sv.starts_with(&info.version),
            "short version should start with crate version: {sv}"
        );
    }

    #[test]
    fn full_version_contains_built() {
        let fv = get_full_version();
        assert!(
            fv.contains("built"),
            "full version should contain 'built': {fv}"
        );
    }

    #[test]
    fn is_release_reflects_version() {
        // The workspace version is "0.1.0" which is not "dev" and not dirty
        assert!(is_release());
    }

    #[test]
    fn version_info_serializes_to_json() {
        let info = get_version_info();
        let json = serde_json::to_string(&info).expect("serialize");
        let restored: VersionInfo = serde_json::from_str(&json).expect("deserialize");
        assert_eq!(info, restored);
    }

    #[test]
    fn display_matches_full_version() {
        let info = get_version_info();
        assert_eq!(info.to_string(), get_full_version());
    }
}
