# Releasing

Release independently versioned crates from the split `core/` and `contrib/` workspaces. Examples are validated but not published. Follow [SemVer](policy/SEMVER.md) and [deprecation](policy/DEPRECATION.md).

## Quickstart

Preview and validate on clean `main`, let Toven stage the bump, then land preparation through a reviewed PR. After merge, the authorized maintainer signs and verifies the umbrella tag before publishing the GitHub Release that triggers CI.

```sh
make release-plan
make release-status
make check
make deny
make release-readiness
make release-coverage
make publish-dry-run
```

Read the phase-specific requirements below before mutating. Published tags and crate versions are immutable.

## Prerequisites

Use a clean, current `main` and maintainer access. Setup tools through `make setup` and `scripts/setup.sh --release` when needed: Git, gh, Cargo, nextest, deny, audit, llvm-cov, cyclonedx, cosign, and the pinned Toven binary.

`CARGO_REGISTRY_TOKEN` is required for actual crates.io publication. CI skips publication when it is absent; skipped publication is not a completed release.

## Flow at a glance

| Phase | Owner and result |
|---|---|
| Bump | Toven stages changed-crate versions, dependency floors, locks, and README pins; maintainer lands a reviewed PR |
| Tag | Maintainer creates and verifies the signed umbrella `vX.Y.Z` tag on merged `main` |
| Publish | Toven verifies the maintainer tag and publishes crates in dependency order |

`entrypoint = "maintainer"` and `push_branch = false` preserve this ownership. Toven does not create, move, push, or verify the signature of the umbrella tag. Never push the bump commit directly to protected `main`.

## 1. Decide the version

Inspect tags and changes since the intended baseline. Before 1.0, breaking changes require a minor bump; otherwise patch. Use [versioning](VERSIONING.md) for independent crate and cascade rules.

## 2. Update the CHANGELOG

**Do this after the bump**, which requires a clean tree. Move populated `[Unreleased]` notes into a dated `[vX.Y.Z]` section, then retain a fresh empty `[Unreleased]` and update comparison links. Notes summarize outcomes and identify breaks. Do not cut an empty release.

## 3. Preview the release

`make release-plan` and `make release-status` show versions, tags, reasons, and publication order. Changed crates and required dependents bump; untouched crates keep their version. Toven owns manifest/floor changes and README pin synchronization; do not hand-edit release versions.

## 4. Pre-flight checks

Run the quickstart gates and `make release-sbom`. Also review the project and confirm measured release coverage and dependency policy pass. Fix failed checks before publication.

### First-release note

A dry-run cannot resolve unpublished internal dependencies from crates.io. It may report blocked-by-unpublished-dependency. Full build/test/docs/audit/coverage checks plus the reviewed dependency order are prerequisites; actual publication must verify each predecessor before its dependents.

## 5. Cut and publish the release

### Phase 1 — Bump (reviewed PR into `main`)

Run `make release-bump` on **clean `main` before editing the changelog**. The configured branch channel maps `main` to `alpha`; an unmapped release branch can finalize the prerelease unintentionally.

The bump is stage-only: no commit, tag, push, or publication. Its `on_resolved` hook synchronizes README version pins alongside manifests and locks.

After rotating the changelog, create a release branch carrying those changes. Review and stage only the release-owned files, commit with signing, and open the authorized PR. Merge only after acceptance passes.

### Phase 2 — Tag (on the merged commit)

On current, clean `main`, the authorized maintainer runs:

```sh
git tag -s vX.Y.Z -m "release rskit-suite X.Y.Z"
git verify-tag vX.Y.Z
git push origin vX.Y.Z
make release-tag
```

Confirm both signature and signer before pushing. `make release-tag` checks the tag at HEAD, not its signature.

### Phase 3 — Publish

Publish a GitHub Release using the **existing** signed tag, target `main`, reviewed notes, and the prerelease flag when applicable. The `release.published` event triggers [release.yml](../.github/workflows/release.yml); a tag push alone does not.

CI installs checksum-verified Toven, checks reachability and readiness, gates coverage, rehearses publishing, publishes in dependency order when credentials exist, and attaches signed CycloneDX SBOMs.

For authorized local publication with credentials configured:

```sh
make release-publish
```

## 6. Verify the workflow release

Find the workflow run for the **exact tag and commit**, not merely the latest run. Inspect its status and failed logs; confirm publication was executed rather than skipped.

### Recovery from a partial or failed release

Inspect `toven release status` before recovery. An unchanged partially published train may resume idempotently, skipping existing `name@version` entries. Rerun the failed workflow against the same immutable tag.

Published defects require a new forward-fix version and the full phase sequence. Never replace a crates.io version, move/delete a published tag, or force-push over it. Yanking discourages new resolution but does not delete a version.

## 7. Verify on crates.io and docs.rs

Check every expected crate/version, including [rskit-suite on crates.io](https://crates.io/crates/rskit-suite) and [docs.rs](https://docs.rs/rskit-suite). Confirm signed SBOM assets and inspect docs.rs build failures before calling the release complete.

## 8. Announce

Update release-facing project references and notify affected sibling consumers using full repository URLs.

## Hotfix releases

Use the same reviewed bump/tag/publish flow for the intended historical line. Add dated notes without discarding unrelated `[Unreleased]` work.

## Pre-releases

Use the configured alpha channel and mark the hosted Release as a prerelease. Verify manifest, tag, and registry versions agree.
