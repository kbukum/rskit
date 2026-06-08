# Versioning Guide

This document explains the versioning and compatibility policy for rskit. For the mechanical release runbook, use [`RELEASING.md`](RELEASING.md).

## Workspace model

rskit is published from two Cargo workspaces:

- `core/Cargo.toml` contains the foundation crates and the `rskit` facade.
- `contrib/Cargo.toml` contains adapter crates.

`examples/Cargo.toml` is validated by CI and release gates, but examples are not published to crates.io.

There is intentionally no root `Cargo.toml`.

## Lock-step versioning

Publishable crates currently share one lock-step version during the `0.x` phase. This keeps the first releases predictable while the API surface is still settling.

The lock-step convention is a repository policy, not a Cargo requirement. Consumers should still depend on the specific crates they use and pin versions normally.

## Version format

Versions follow SemVer 2.0.0:

```text
MAJOR.MINOR.PATCH[-PRERELEASE][+BUILDMETADATA]
```

Examples:

- `0.1.0-alpha.1` — cautious first prerelease.
- `0.1.0` — first final minor release.
- `0.3.0-beta.2` — beta release.
- `0.3.0-rc.1` — release candidate.
- `1.0.0` — first stable major release.

Cargo orders prerelease identifiers according to SemVer (`alpha < beta < rc < final`).

## Workspace inheritance

Each split workspace declares shared package metadata once:

```toml
[workspace.package]
version = "0.1.0-alpha.1"
edition = "2024"
rust-version = "1.91"
```

Member crates inherit the shared version:

```toml
[package]
name = "rskit-errors"
version.workspace = true
edition.workspace = true
rust-version.workspace = true
```

Internal workspace dependencies also include explicit versions so crates can be published to crates.io:

```toml
rskit-errors = { path = "rskit-errors", version = "0.1.0-alpha.1" }
```

Contrib crates that depend on core crates use paths relative to `contrib/`:

```toml
rskit-errors = { path = "../core/rskit-errors", version = "0.1.0-alpha.1" }
```

## Release mechanics

A single `v*` GitHub Release covers a repository release while crates remain lock-step. The exact version-bump, changelog, release publication, publish dry-run, SBOM, signing, and crates.io publishing steps live in [`RELEASING.md`](RELEASING.md). Keep this guide policy-focused so contributors do not have to reconcile duplicate runbooks.

## Compatibility policy

### Pre-1.0 (`0.x.y`)

- **MINOR** (`0.X.0`) bumps may contain breaking API changes.
- **PATCH** (`0.x.Y`) bumps are bug fixes, performance improvements, internal refactors, and non-breaking additions.
- Every breaking change must be documented in `CHANGELOG.md`.
- rskit will not promote crates to `1.0.0` until the public API is settled and maintainers are ready to commit to the full `1.x` compatibility contract.

### Post-1.0 (`1.x.y`)

See [`policy/SEMVER.md`](policy/SEMVER.md) for the full post-1.0 contract.

## Consumer examples

Use the facade when you want one crate with opt-in features:

```toml
[dependencies]
rskit = { version = "0.1.0-alpha.1", features = ["server", "database", "messaging"] }
```

Or depend on focused crates directly:

```toml
[dependencies]
rskit-errors = "0.1.0-alpha.1"
rskit-resilience = "0.1.0-alpha.1"
rskit-worker = "0.1.0-alpha.1"
```

## MSRV

The MSRV is declared by `[workspace.package].rust-version`. MSRV bumps are breaking and ship in MINOR releases before 1.0, or MAJOR releases after 1.0.

The README badge documents the current MSRV. `rust-toolchain.toml` pins the development and CI toolchain, which may be newer than the MSRV.

## Rules of thumb

1. Release the split workspaces together until per-crate release cadence is formally adopted.
2. Treat `0.x` minor releases as the place for documented breaking changes.
3. Never force-push release tags.
4. Keep release mechanics in [`RELEASING.md`](RELEASING.md).
5. Fix forward with a new version if crates.io publishing partially succeeds; published crate versions are immutable.

## References

- [SemVer 2.0.0](https://semver.org/spec/v2.0.0.html)
- [Cargo SemVer compatibility](https://doc.rust-lang.org/cargo/reference/semver.html)
- [Cargo workspaces](https://doc.rust-lang.org/cargo/reference/workspaces.html)
- [RELEASING.md](RELEASING.md)
- [policy/SEMVER.md](policy/SEMVER.md)
- [policy/DEPRECATION.md](policy/DEPRECATION.md)
