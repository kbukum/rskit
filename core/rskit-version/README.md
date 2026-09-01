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
    println!("{:?}", info.build_date);  // e.g. Some("2024-01-15")
    println!("{}", info.is_dirty);      // true when built from a dirty working tree

    // A `-dirty` marker is appended when the working tree was dirty at build time.
    println!("{}", get_short_version()); // "0.1.0-alpha.1-a1b2c3d[-dirty]"
    println!("{}", get_full_version());  // "0.1.0-alpha.1-a1b2c3d[-dirty] (built 2024-01-15T10:30:00Z)"
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
| `git_dirty` | `git status --porcelain` (any output ⇒ dirty) |
| `build_time` | UTC timestamp at build time (captured as Unix epoch, formatted to RFC 3339) |
| `rust_version` | `rustc --version` |

`SOURCE_DATE_EPOCH` overrides the build timestamp for reproducible builds; when it is set to a
value that is not a non-negative integer the build fails rather than silently using the wall clock.

## Key Types & Functions

| Name | Description |
|------|-------------|
| `VersionInfo` | Struct with version, git_commit, git_branch, build_time, build_date, rust_version, is_release, is_dirty |
| `get_version_info()` | Returns full `VersionInfo` |
| `get_short_version()` | Returns `version-commit[-dirty]` string |
| `get_full_version()` | Returns detailed version string with branch, dirty marker, and build time |
| `is_release()` | `true` when version is not `"dev"` and does not contain `"dirty"` |
| `package_semver()` | Returns the package version parsed as SemVer |
| `semver` | SemVer parsing and requirement helpers backed by the `semver` crate |

## Cross-Kit Consistency

This crate mirrors the API of:
- **gokit** — `version.VersionInfo` / `version.GetVersionInfo()`
- **pykit** — `pykit_version.VersionInfo` / `pykit_version.get_version_info()`

---

[⬅ Back to main README](../../README.md)
