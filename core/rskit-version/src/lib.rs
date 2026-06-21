//! Build-time version and git metadata for rskit.
//!
//! Version, git commit, branch, and build time are captured at compile time
//! via a `build.rs` script that runs `git` commands and emits `cargo:rustc-env`
//! variables. The build timestamp is captured as a Unix epoch and formatted to
//! RFC 3339 by the library, avoiding any external `date` command.
//!
//! # Quick Start
//!
//! ```rust
//! use rskit_version::{get_version_info, get_short_version, get_full_version, is_release};
//!
//! let info = get_version_info();
//! println!("{}", info.version);      // e.g. "0.1.0-alpha.1"
//! println!("{}", info.git_commit);   // e.g. "a1b2c3d..."
//! println!("{}", get_short_version()); // "0.1.0-alpha.1-a1b2c3d"
//! println!("{}", get_full_version()); // "0.1.0-alpha.1-a1b2c3d (built 2024-01-15T10:30:00Z)"
//! ```

#![warn(missing_docs)]

pub mod schema;
pub mod semver;

use serde::{Deserialize, Serialize};
use std::fmt;

const MAIN_BRANCH: &str = "main";
const MASTER_BRANCH: &str = "master";
const SHORT_COMMIT_LEN: usize = 7;
const GIT_BRANCH: &str = env!("GIT_BRANCH");

/// Immutable snapshot of build/version metadata. Compatible with gokit and pykit `VersionInfo`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct VersionInfo {
    /// Crate version from `Cargo.toml` (e.g. `"0.1.0-alpha.1"`).
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
        f.write_str(&self.full_version())
    }
}

impl VersionInfo {
    /// Return the package version without build metadata.
    pub fn package_version(&self) -> &str {
        &self.version
    }

    /// Return the package version parsed as a semantic version, if valid.
    pub fn semver(&self) -> Option<semver::Version> {
        semver::parse_version(&self.version)
    }

    /// Return whether this package version matches a semantic version requirement.
    ///
    /// Returns `None` if either the version or requirement string is invalid.
    pub fn matches_requirement(&self, requirement: &str) -> Option<bool> {
        semver::matches_requirement(&self.version, requirement)
    }

    /// Return the short git commit hash, if available.
    pub fn short_git_commit(&self) -> Option<&str> {
        if self.git_commit.is_empty() {
            None
        } else {
            Some(short_commit(&self.git_commit))
        }
    }

    /// Returns a concise version string: `{version}[-{short_commit}]`.
    pub fn short_version(&self) -> String {
        self.short_git_commit().map_or_else(
            || self.version.clone(),
            |commit| format!("{}-{commit}", self.version),
        )
    }

    /// Returns a detailed version string with optional branch and build time.
    pub fn full_version(&self) -> String {
        let mut parts = vec![self.version.clone()];

        if let Some(commit) = self.short_git_commit() {
            parts.push(commit.to_owned());
        }

        if !self.git_branch.is_empty()
            && self.git_branch != MAIN_BRANCH
            && self.git_branch != MASTER_BRANCH
        {
            parts.push(self.git_branch.clone());
        }

        let mut version = parts.join("-");

        if !self.build_time.is_empty() {
            use std::fmt::Write;
            let _ = write!(version, " (built {})", self.build_time);
        }

        version
    }
}

fn short_commit(commit: &str) -> &str {
    commit
        .char_indices()
        .nth(SHORT_COMMIT_LEN)
        .map_or(commit, |(idx, _)| &commit[..idx])
}

/// Return the Cargo package version for this crate.
pub const fn package_version() -> &'static str {
    env!("CARGO_PKG_VERSION")
}

/// Return the Cargo package version parsed as a semantic version, if valid.
pub fn package_semver() -> Option<semver::Version> {
    semver::parse_version(package_version())
}

/// Returns comprehensive version information collected at compile time.
pub fn get_version_info() -> VersionInfo {
    let version = package_version().to_owned();
    let git_commit = env!("GIT_COMMIT").to_owned();
    let git_branch = if GIT_BRANCH.is_empty() {
        String::new()
    } else {
        GIT_BRANCH.to_owned()
    };
    let build_time = env!("BUILD_EPOCH")
        .parse::<i64>()
        .ok()
        .and_then(rskit_util::time::format_rfc3339)
        .unwrap_or_default();
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
    get_version_info().short_version()
}

/// Returns a detailed version string with optional branch and build time.
///
/// Format: `{version}[-{short_commit}][-{branch}] (built {time})`
/// Branches named `main` or `master` are omitted.
pub fn get_full_version() -> String {
    get_version_info().full_version()
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
        // Pre-release versions are still release builds; only dev/dirty builds are not.
        assert!(is_release());
    }

    #[test]
    fn display_matches_full_version() {
        let info = get_version_info();
        assert_eq!(info.to_string(), get_full_version());
    }

    #[test]
    fn package_version_parses_as_semver() {
        let version = package_semver().expect("workspace package version should be semver");
        assert_eq!(version.major, 0);
        assert!(get_version_info().semver().is_some());
    }

    #[test]
    fn version_info_matches_semver_requirement() {
        let info = get_version_info();
        assert_eq!(info.matches_requirement(">=0.1.0-alpha.1"), Some(true));
        assert_eq!(info.matches_requirement(">=0.1.0"), Some(false));
        assert_eq!(info.matches_requirement("not a requirement"), None);
    }
}
