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

mod info;

pub use info::{
    VersionInfo, get_full_version, get_short_version, get_version_info, is_release, package_semver,
    package_version,
};

#[cfg(test)]
mod tests;
