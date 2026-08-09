# Versioning Guide

This document explains the versioning and compatibility policy for rskit. For the mechanical release runbook, use [`RELEASING.md`](RELEASING.md). For how this model is expected to evolve (and the supporting tooling), see [`VERSIONING-ROADMAP.md`](VERSIONING-ROADMAP.md).

## Workspace model

rskit is published from two Cargo workspaces:

- `core/Cargo.toml` contains the foundation crates and the `rskit-suite` facade package, whose Rust crate name remains `rskit`.
- `contrib/Cargo.toml` contains adapter crates.

`examples/Cargo.toml` is validated by CI and release gates,
but examples are not published to crates.io.

There is intentionally no root `Cargo.toml`.

## Independent per-crate versioning

Each publishable crate carries its **own** `version` and bumps only when it changes (plus the correct cascade). Crates share all other `[workspace.package]` metadata (edition, license, rust-version, authors, repository, homepage, documentation) but **not** the version.

The model uses 0.x SemVer with caret dependency pins:

- A dependent's internal pin (`{ path = "...", version = "x.y.z" }`) is a **caret** requirement, so a dependency **patch** bump is absorbed — **no cascade, no republish**.
- A dependency **minor** bump (the breaking position in 0.x) leaves the caret range, so its in-workspace dependents must move their pin floor and republish.

`core/` and `contrib/` are separate workspaces and release as **independent trains**; tooling operates per workspace.

This is a repository policy, not a Cargo requirement. Consumers should depend on the specific crates they use and pin versions normally.

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

Each split workspace declares shared package metadata once — but **not** the version, which each crate owns:

```toml
[workspace.package]
edition = "2024"
rust-version = "1.91"
```

Member crates carry their own version and inherit the rest:

```toml
[package]
name = "rskit-errors"
version = "0.1.0-alpha.1"
edition.workspace = true
rust-version.workspace = true
```

Internal workspace dependencies include a caret version (floor) so crates can be published to crates.io; `cargo publish` strips `path` and keeps `version`, so the `path` is local-dev convenience only:

```toml
rskit-errors = { path = "rskit-errors", version = "0.1.0-alpha.1" }
```

Contrib crates that depend on core crates use paths relative to `contrib/`:

```toml
rskit-errors = { path = "../core/rskit-errors", version = "0.1.0-alpha.1" }
```

## Release mechanics

[Toven](https://github.com/kbukum/toven) drives releases. It detects the crates changed since each crate's last release tag from the Conventional-Commit history, applies a **patch** bump by default and a **minor** bump for a breaking change, cascades breaking minors to in-workspace dependents, and rewrites caret floors. It is idempotent against the already-published crates.io versions, then publishes only the new `name@version`s in dependency order. The `make release-*` targets delegate to it; the full runbook lives in [`RELEASING.md`](RELEASING.md).

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
rskit-suite = { version = "0.1.0-alpha.1", features = ["server", "database", "messaging"] }
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

1. Preview what will change: `make release-plan`; Toven derives the per-crate bumps and cascade from commit history.
2. Treat `0.x` minor releases as the place for documented breaking changes (they cascade to dependents).
3. Never force-push release tags.
4. Keep release mechanics in [`RELEASING.md`](RELEASING.md).
5. Fix forward with a new version if crates.io publishing partially succeeds; published crate versions are immutable.

## Toven versioning

`toven.toml`'s `[ecosystems.rust.release]` block encodes the independent per-crate, crates.io-targeted model this document describes, and Toven is the canonical driver for it. The read-only `make toven-canary` target runs mutation-free previews (`modules`, `graph`, `release status`, `release plan`) to prove the published binary against this repository ahead of each release.

| Versioning behavior | Toven command | Expected output |
|---|---|---|
| Independent per-crate bump | `toven release plan` | cascade table with a per-crate current → planned version and bump kind |
| Breaking-minor cascade to dependents | `toven release plan` (`dependency cascade` reason column) | dependents shown as cascaded when a dependency takes a breaking bump |
| Registry idempotency (skip already-published) | `toven release status` + `toven release plan` | per-crate published/planned verdicts that keep already-published versions out of the mutation plan |
| Split-workspace discovery (`core`, `contrib`, `examples`) | `toven modules` | all workspace crates discovered under one graph |
| Non-publishable crate exclusion | `toven release status` / `toven release plan` | `agent-demo`, `core-cli`, `media-demo`, and `rskit-fuzz` are discovered by `toven modules` but explicitly `exclude`d from the release, so they never appear in the plan or reach crates.io |
| Clean-tree requirement | `toven release readiness` (`make release-readiness`) | dirty trees fail before release mutation or publish |

`make release-tag` / `make release-publish` apply the manifest edits, `toven release readiness` is the release gate, and the GitHub Release workflow publishes to crates.io through the same binary.

## References

- [SemVer 2.0.0](https://semver.org/spec/v2.0.0.html)
- [Cargo SemVer compatibility](https://doc.rust-lang.org/cargo/reference/semver.html)
- [Cargo workspaces](https://doc.rust-lang.org/cargo/reference/workspaces.html)
- [RELEASING.md](RELEASING.md)
- [policy/SEMVER.md](policy/SEMVER.md)
- [policy/DEPRECATION.md](policy/DEPRECATION.md)
