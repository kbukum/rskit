# rskit-version

Build-time version and git metadata for rskit, captured at compile time via `build.rs`.

## Installation

```toml
[dependencies]
rskit-version = { path = "../rskit-version" }
```

## Quick Start

```rust
use rskit_version::{get_version_info, get_short_version, get_full_version, package_semver};

fn main() {
    let info = get_version_info();
    println!("{}", info.version);       // e.g. "0.1.0-alpha.1"
    println!("{}", info.git_commit);    // e.g. "a1b2c3d4..."
    println!("{}", info.git_branch);    // e.g. "main"
    println!("{}", info.rust_version);  // e.g. "rustc 1.97.0 ..."

    println!("{}", get_short_version()); // "0.1.0-alpha.1-a1b2c3d"
    println!("{}", get_full_version());  // "0.1.0-alpha.1-a1b2c3d (built 2024-01-15T10:30:00Z)"
    println!("{:?}", package_semver());   // Some(Version { major: 0, minor: 1, patch: 0 })
}
```

## How It Works

The `build.rs` script runs at compile time to capture:

| Variable | Source |
|----------|--------|
| `version` | `CARGO_PKG_VERSION` from `Cargo.toml` |
| `git_commit` | `git rev-parse HEAD` |
| `git_branch` | `git rev-parse --abbrev-ref HEAD` |
| `build_time` | UTC timestamp at build time (captured as Unix epoch, formatted to RFC 3339) |
| `rust_version` | `rustc --version` |

## Key Types & Functions

| Name | Description |
|------|-------------|
| `VersionInfo` | Struct with version, git_commit, git_branch, build_time, rust_version, is_release |
| `get_version_info()` | Returns full `VersionInfo` |
| `get_short_version()` | Returns `version-commit` string |
| `get_full_version()` | Returns detailed version string with branch and build time |
| `is_release()` | `true` when version is not `"dev"` and does not contain `"dirty"` |
| `package_semver()` | Returns the package version parsed as SemVer |
| `semver` | SemVer parsing and requirement helpers backed by the `semver` crate |

## Cross-Kit Consistency

This crate mirrors the API of:
- **gokit** — `version.VersionInfo` / `version.GetVersionInfo()`
- **pykit** — `pykit_version.VersionInfo` / `pykit_version.get_version_info()`

---

[⬅ Back to main README](../../README.md)
