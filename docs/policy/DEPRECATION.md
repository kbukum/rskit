# Deprecation Policy

This policy applies once a crate reaches `1.0.0`. While in `0.x.y` we may
remove APIs in any MINOR release (see [`SEMVER.md`](SEMVER.md)), but we still
try to follow the spirit of this document where practical.

## Lifecycle of a deprecated API

```
   stable ──► deprecated ──► removed
              ↑           ↑
              MINOR       MAJOR
              release     release (≥ 1 MINOR later)
```

1. **Deprecation** — the API is marked deprecated in a MINOR release.
2. **Cohabitation** — the new and old APIs coexist for at least one full
   MINOR release cycle (target: 6 months of calendar time, minimum: 1 MINOR).
3. **Removal** — the deprecated API is removed in the next MAJOR release.

We never remove a deprecated API in a PATCH or MINOR release after `1.0.0`.

## How we mark deprecation

Every deprecated symbol carries:

1. A `#[deprecated]` attribute with `since` and `note` — recognised by
   `rustc`, `clippy`, IDEs, and `docs.rs`.
2. The version it was deprecated in.
3. The replacement (or `no replacement, will be removed in vX.Y.Z`).

```rust
/// Create an auth checker.
///
/// Use [`new_verifier`] which threads `tracing::Span`.
#[deprecated(
    since = "1.2.0",
    note = "use `new_verifier`; will be removed in v2.0.0"
)]
pub fn new_auth_checker(cfg: Config) -> Checker { /* … */ }
```

4. A CHANGELOG entry under `### Deprecated` for the release that introduced
   the deprecation.
5. (Where helpful) a runtime `tracing::warn!` from the function's first call,
   gated by `std::sync::OnceLock<()>`, naming the replacement. This is
   optional — only do it for hot-path APIs where a doc comment is easy to
   miss.

## What counts as a deprecation-eligible change

- Removing a public function, method, struct, enum, trait, type alias, or
  constant.
- Removing a public field from a non-`#[non_exhaustive]` struct.
- Adding a non-defaulted method to a public trait.
- Adding a variant to a non-`#[non_exhaustive]` public enum.
- Tightening a parameter or return type.
- Bumping the MSRV.
- Changing observable runtime behaviour in a way callers might depend on.
- Removing or renaming a feature flag.

The following are **not** deprecations and may ship in a single MINOR/PATCH:

- Adding a new method to a struct or trait (with a default impl for traits).
- Adding a new field to a `#[non_exhaustive]` struct.
- Adding a new variant to a `#[non_exhaustive]` enum.
- Tightening behaviour to fix a documented bug.
- Adding a new optional feature flag.

## Security exception

A vulnerability fix may break API in a PATCH release if no compatible fix
exists. This is the only exception. Such releases are flagged with
`SECURITY:` in the CHANGELOG and announced via GitHub Security Advisories
and (where applicable) [RUSTSEC](https://rustsec.org/).

## Deprecation checklist for maintainers

Before merging a deprecation PR:

- [ ] `#[deprecated(since = "X.Y.Z", note = "…")]` attribute on the symbol.
- [ ] CHANGELOG `### Deprecated` entry under `[Unreleased]`.
- [ ] Replacement API exists and is documented.
- [ ] If the replacement requires a non-trivial migration, add a
      `## Migration` block to the CHANGELOG entry showing before/after.
- [ ] Removal date / version recorded in `docs/policy/DEPRECATIONS.csv`
      (sortable list — create on first deprecation).
- [ ] `clippy::deprecated_in_future` check passes (the deprecation compiles
      cleanly in the workspace).
