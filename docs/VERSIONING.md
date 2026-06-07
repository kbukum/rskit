# Versioning Guide

This document explains how versioning works in the rskit workspace.

## Why versioning matters here

rskit is a **Cargo workspace** with one facade crate (`rskit`) and 40+
sub-crates (`rskit-{name}`). Each crate is published independently to
crates.io but currently all share the **same version** for predictability
during the `0.x` phase via `[workspace.package]` inheritance. The lock-step
convention is convenience, not contract — consumers should pin per crate.

## Version Format

```
MAJOR.MINOR.PATCH[-PRERELEASE][+BUILDMETADATA]
```

Examples:
- `0.1.0` — first minor release
- `1.0.0` — first major release
- `1.2.3` — standard release
- `0.3.0-rc.1` — release candidate
- `0.3.0-beta.2` — beta

Cargo follows SemVer 2.0.0 strictly; pre-release identifiers are ordered
according to the spec (`alpha < beta < rc < <release>`).

## Workspace inheritance

Each workspace declares shared package metadata once in its workspace manifest.
For example, `core/Cargo.toml` uses paths relative to `core/`:

```toml
# core/Cargo.toml
[workspace.package]
version = "0.1.0"
edition = "2024"
rust-version = "1.91"

[workspace.dependencies]
rskit-errors = { path = "rskit-errors", version = "0.1.0" }
# … core and contrib members follow the same pattern
```

Each member crate inherits via:

```toml
[package]
name = "rskit-errors"
version.workspace = true
edition.workspace = true
rust-version.workspace = true
```

Bump the workspace version with `cargo set-version --workspace X.Y.Z`
(`cargo-edit`).

## Tagging

A single Git tag covers the whole workspace:

```sh
git tag -s -a v0.1.0 -m "v0.1.0"
git push origin v0.1.0
```

The release workflow then publishes every workspace crate from `core/` and
`contrib/`, plus the facade, in lock-step.

Publishing order is derived from Cargo metadata by
[`scripts/publish-dry-run.sh`](../scripts/publish-dry-run.sh), which publishes
internal path dependencies before crates that depend on them.

## Compatibility Policy

### Pre-1.0 (`0.x.y`)

- **MINOR** (`0.X.0`) bumps **may** contain breaking API changes. Every
  break is documented in `CHANGELOG.md` under
  `### Changed (Breaking API Changes)`.
- **PATCH** (`0.x.Y`) bumps are bug fixes, performance improvements,
  internal refactors, and **non-breaking** additions.
- We will not promote a crate to `1.0.0` until its public API is settled
  and we are willing to commit to the full `1.x` compatibility contract for
  at least 12 months.

### Post-1.0 (`1.x.y` and beyond)

See [`policy/SEMVER.md`](policy/SEMVER.md) for the full post-1.0 contract.

## Using Versioned Crates

```toml
# Use a specific version (lock-step)
[dependencies]
rskit            = "0.1"
rskit-errors     = "0.1"
rskit-resilience = "0.1"

# Or use the facade with feature flags
rskit = { version = "0.1", features = ["server", "database", "messaging"] }
```

```rust
use rskit::errors::{AppError, ErrorCode};
use rskit::resilience::CircuitBreaker;
```

## MSRV (Minimum Supported Rust Version)

The MSRV is declared by `[workspace.package] rust-version`. MSRV bumps are
**breaking** and ship in MINOR releases (pre-1.0) or MAJOR releases
(post-1.0). The current MSRV is documented in the README badge. The
`rust-toolchain.toml` file pins the development and CI toolchain, which may be
newer than the MSRV.

## Best Practices

1. **Always tag the workspace as a whole** until per-crate versioning is
   formally adopted post-1.0.
2. **Follow SemVer strictly** — breaking changes = MAJOR (after 1.0) or
   MINOR (in 0.x).
3. **Update CHANGELOG.md** under `[Unreleased]` before tagging.
4. **Test before tagging** — run `make release-readiness`,
   `make release-coverage`, and `make publish-dry-run`.
5. **Never force-push tags.**
6. **Use pre-release tags for testing** — `v0.2.0-beta.1`.

## References

- [SemVer 2.0.0](https://semver.org/spec/v2.0.0.html)
- [Cargo SemVer compatibility](https://doc.rust-lang.org/cargo/reference/semver.html)
- [Cargo workspaces](https://doc.rust-lang.org/cargo/reference/workspaces.html)
- [policy/SEMVER.md](policy/SEMVER.md)
- [policy/DEPRECATION.md](policy/DEPRECATION.md)
- [RELEASING.md](RELEASING.md)
