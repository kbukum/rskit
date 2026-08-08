---
name: release
description: >-
    Cut a release of the rskit multi-workspace monorepo — decide the semver bump, update the
    CHANGELOG, let Toven derive independent per-crate version bumps, run the full pre-release gates
    and supply-chain sweep, and publish to crates.io in dependency order. Use when preparing or
    publishing an rskit release or checking release readiness.
---

# Releasing rskit

rskit has **split Cargo workspaces** (`core/`, `contrib/`, `examples/`) and **no root `Cargo.toml`**. Crates are versioned **independently** and published to crates.io in dependency order. [Toven](https://github.com/kbukum/toven) is the canonical release driver — it owns the version bump, release commit, signed tagging, crates.io publication, SBOM, and the readiness preflight; the `make release-*` targets delegate to it. Full details live in [`docs/RELEASING.md`](../../../docs/RELEASING.md), [`docs/VERSIONING.md`](../../../docs/VERSIONING.md), [`docs/policy/SEMVER.md`](../../../docs/policy/SEMVER.md), and [`docs/policy/DEPRECATION.md`](../../../docs/policy/DEPRECATION.md).

## Prerequisites

- Listed in `MAINTAINERS.md` with push access to `kbukum/rskit`.
- On `main`, clean working tree. Run `make setup` and `scripts/setup.sh --release`; ensure `git`, `gh`, `cargo`, `cargo-nextest`, `cargo-deny`, `cargo-audit`, `cargo-llvm-cov`, `cargo-cyclonedx`, and `cosign` are on `$PATH`.
- `CARGO_REGISTRY_TOKEN` configured for crates.io publishing (the workflow skips publishing when it is absent).

## Step 1 — Full pre-release gate

A release is the one time to run the **complete** gates rather than the affected set:

```bash
make check                  # fmt-check + lint + build + test-nextest + test-doc + test-python
make release-readiness      # Toven fail-closed preflight (clean tree, changelog, deny + audit)
make release-coverage       # per-package coverage gate (default line coverage >=90%)
```

Also run the `review` project audit in a fresh agent before a release. Treat green gates as necessary but not sufficient.

## Step 2 — Decide the version

```bash
git tag --sort=-v:refname | head -1
git log --oneline $(git describe --tags --abbrev=0)..HEAD
```

Use [`docs/policy/SEMVER.md`](../../../docs/policy/SEMVER.md). While in `0.x`: a breaking change in the `[Unreleased]` CHANGELOG section bumps **MINOR**; otherwise **PATCH**.

## Step 3 — Update the CHANGELOG

1. Open `CHANGELOG.md`.
2. Replace `## [Unreleased]` with `## [vX.Y.Z] - YYYY-MM-DD`.
3. Add a fresh empty `## [Unreleased]` section above it.
4. If the new `[vX.Y.Z]` section is empty, **refuse to release** — nothing to ship.
5. Update the link references at the bottom if present.

## Step 4 — Preview the release

Toven derives per-crate versions, tags, and publish order from the dependency graph and the Conventional-Commit history since each crate's last tag — you never hand-edit versions. Preview before mutating:

```bash
make release-plan     # bumped versions, tags, changelog, and publish order (read-only)
make release-status   # declared vs published vs tagged versions (read-only)
```

Toven writes the manifest bumps and refreshes caret floors when it cuts the release (Step 5); the plan is idempotent against the already-published versions.

## Step 5 — SBOM and publish

Toven cuts and publishes. Preview, then cut:

```bash
make release-sbom            # CycloneDX SBOMs under target/sbom
make publish-dry-run         # rehearse the full pipeline in dependency order (read-only)
make release-tag             # bump manifests, commit, and create signed tags
make release-publish         # full pipeline: commit, tag, push, publish to crates.io (idempotent)
```

In CI, publishing a GitHub Release triggers `.github/workflows/release.yml`, which runs the same Toven-driven readiness, dry-run, SBOM, and publish steps through the pinned binary. Follow the remaining steps in `docs/RELEASING.md` (GitHub release with notes from the CHANGELOG section, signed artifacts). CI actions must be SHA-pinned.

## Guardrails

- **Never** run destructive git commands (`reset --hard`, `checkout -- .`, `clean`) on uncommitted work without explicit permission.
- Per repo workflow, the agent prepares the branch/CHANGELOG/bump edits; **the maintainer commits, pushes, and runs the actual publish** unless explicitly asked otherwise. Open a PR only when explicitly requested, following the PR template.
- Reference other-repo items with full URLs, never bare `#123`.
