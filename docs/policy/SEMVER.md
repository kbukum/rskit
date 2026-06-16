# Semantic Versioning Policy

`rskit` follows [Semantic Versioning 2.0.0](https://semver.org/spec/v2.0.0.html) and the [Cargo SemVer compatibility rules](https://doc.rust-lang.org/cargo/reference/semver.html), with the workspace clarifications below.

## Versioning surface

`rskit` is published from split Cargo workspaces: `core/Cargo.toml` for foundation crates and the `rskit-suite` facade package (Rust crate name `rskit`), and `contrib/Cargo.toml` for adapter crates. **Each crate is versioned independently**: it carries its own `[package].version` and bumps only when it changes, plus the caret cascade. Consumers should pin per crate.

## Pre-1.0 (`0.x.y`)

While the project is in `0.x.y`:

- **MINOR** (`0.X.0`) bumps **may** contain breaking API changes. Every break is documented in `CHANGELOG.md` under `### Changed (Breaking API Changes)` for the affected crate.
- **PATCH** (`0.x.Y`) bumps are bug fixes, performance improvements, internal refactors, and **non-breaking** additions. PATCH releases never break the public API.
- We will not promote a crate to `1.0.0` until its public API is settled and we are willing to commit to the full `1.x` compatibility contract for at least 12 months.

## Post-1.0 (`1.x.y` and beyond)

- **MAJOR** (`X.0.0`) — breaking change to a stable public API. Requires a deprecation cycle (see [`DEPRECATION.md`](DEPRECATION.md)) of at least one MINOR release before the breaking change ships.
- **MINOR** (`x.Y.0`) — backwards-compatible additions and behaviour changes. Marking an API as deprecated is a MINOR change.
- **PATCH** (`x.y.Z`) — backwards-compatible bug and security fixes only.

## What counts as the public API

For a Rust crate, the public API is everything reachable from the crate root that is `pub` (and not behind a `#[doc(hidden)]` or unstable feature flag). Cargo's [SemVer compatibility rules](https://doc.rust-lang.org/cargo/reference/semver.html) are the authoritative reference. Briefly:

- Public functions, methods, traits, structs, enums, type aliases, constants, and statics.
- The signatures and observable behaviour of all of the above.
- Documented invariants in `///` doc comments.
- The set of variants on a public enum (unless marked `#[non_exhaustive]`).
- The set of methods on a public trait — adding a non-defaulted method is a break.
- The set of public fields on a struct (unless marked `#[non_exhaustive]`).
- The MSRV (`rust-version` in `Cargo.toml`).
- The default and additive feature flag set.

The following are explicitly **not** part of the public API and may change in any release:

- Anything in a `#[doc(hidden)]` module or behind an unstable feature.
- Items behind `pub(crate)` or `pub(super)`.
- Items in `*-internal` crates.
- Generated code (e.g., `tonic_build` output) when the upstream `.proto` changes.
- Dependency versions, beyond the documented MSRV.
- The exact Cargo lockfiles of the split workspaces.

## Workspace-level version skew

Sub-crates are expected to sit at different versions: a focused fix ships as e.g. `rskit-storage 0.2.1` while unaffected crates stay put. Caret pins absorb such patch bumps in dependents, so version skew is the normal steady state, not a temporary condition to reconcile.

## Pre-release identifiers

Pre-releases use SemVer suffixes: `0.3.0-rc.1`, `0.3.0-beta.2`. Pre-release tags do not require CHANGELOG entries but **must** be reproducible builds (no moving toolchain reference, no floating action SHAs).

## See also

- [`DEPRECATION.md`](DEPRECATION.md) — how we deprecate and eventually remove APIs.
- [`../RELEASING.md`](../RELEASING.md) — the mechanical release process.
- [`../../GOVERNANCE.md`](../../GOVERNANCE.md) — who can cut a release and how.
