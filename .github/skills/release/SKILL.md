---
name: release
description: >-
    Cut a release of the rskit multi-workspace monorepo — decide the semver bump, update the
    CHANGELOG, apply independent per-crate version bumps, run the full pre-release gates and
    supply-chain sweep, and publish to crates.io in dependency order. Use when preparing or
    publishing an rskit release or checking release readiness.
---

# Releasing rskit

rskit has **split Cargo workspaces** (`core/`, `contrib/`, `examples/`) and **no root `Cargo.toml`**. Crates are versioned **independently** and published to crates.io in dependency order via the `rskit_tool` release tooling (driven through `make`). Full details live in [`docs/RELEASING.md`](../../../docs/RELEASING.md), [`docs/VERSIONING.md`](../../../docs/VERSIONING.md), [`docs/policy/SEMVER.md`](../../../docs/policy/SEMVER.md), and [`docs/policy/DEPRECATION.md`](../../../docs/policy/DEPRECATION.md).

## Prerequisites

- Listed in `MAINTAINERS.md` with push access to `kbukum/rskit`.
- On `main`, clean working tree. Run `make setup` and `scripts/setup.sh --release`; ensure `git`, `gh`, `cargo`, `cargo-nextest`, `cargo-deny`, `cargo-audit`, `cargo-llvm-cov`, `cargo-cyclonedx`, and `cosign` are on `$PATH`.
- `CARGO_REGISTRY_TOKEN` configured for crates.io publishing (the workflow skips publishing when it is absent).

## Step 1 — Full pre-release gate

A release is the one time to run the **complete** gates rather than the affected set:

```bash
make check                  # fmt-check + lint + build + test-nextest + test-doc + test-python
make release-readiness      # supply-chain + API sweep (cargo-deny + cargo-audit)
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

## Step 4 — Bump versions per workspace

Crates bump independently — each only when it changed, plus the correct in-workspace cascade. Use the tooling, never hand-edit manifests:

```bash
make release-bump W=core DRY=1                       # preview the plan, no writes
make release-bump W=core MINOR="rskit-httpclient"    # apply; flag breaking crates with MINOR
make release-bump W=contrib
```

The tool is idempotent against the crates.io max published version and performs **no network writes** (manifest edits only).

## Step 5 — SBOM and publish

```bash
make release-sbom            # CycloneDX SBOMs under target/sbom
make publish-dry-run         # dry-run publish in dependency order
make release-publish         # publish to crates.io (idempotent, rate-aware, resumable)
```

Then follow the remaining steps in `docs/RELEASING.md` (git tag, GitHub release with notes from the CHANGELOG section, signed artifacts). CI actions must be SHA-pinned.

## Guardrails

- **Never** run destructive git commands (`reset --hard`, `checkout -- .`, `clean`) on uncommitted work without explicit permission.
- Per repo workflow, the agent prepares the branch/CHANGELOG/bump edits; **the maintainer commits, pushes, and runs the actual publish** unless explicitly asked otherwise. Open a PR only when explicitly requested, following the PR template.
- Reference other-repo items with full URLs, never bare `#123`.
