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
    /// Rust compiler version string (e.g. `"rustc 1.97.0 ..."`).
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
    pub fn semver(&self) -> Option<crate::semver::Version> {
        crate::semver::parse_version(&self.version)
    }

    /// Return whether this package version matches a semantic version requirement.
    ///
    /// Returns `None` if either the version or requirement string is invalid.
    pub fn matches_requirement(&self, requirement: &str) -> Option<bool> {
        crate::semver::matches_requirement(&self.version, requirement)
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
pub fn package_semver() -> Option<crate::semver::Version> {
    crate::semver::parse_version(package_version())
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
/// Format: `{version}[-{short_commit}][-{branch}] (built {time})` Branches named `main`
/// or `master` are omitted.
pub fn get_full_version() -> String {
    get_version_info().full_version()
}

/// Returns `true` when this build represents a release (not dev, not dirty).
pub fn is_release() -> bool {
    get_version_info().is_release
}
