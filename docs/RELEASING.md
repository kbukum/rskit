# Releasing

The mechanical steps to cut a release of `rskit`. For *what* counts as a breaking change vs a feature vs a fix, see [`policy/SEMVER.md`](policy/SEMVER.md) and [`policy/DEPRECATION.md`](policy/DEPRECATION.md).

## Prerequisites

- You are listed in `MAINTAINERS.md` and have push access to `kbukum/rskit`.
- Your local clone is on `main` with no uncommitted changes.
- `git`, `gh`, `cargo`, `cargo-nextest`, `cargo-deny`, `cargo-audit`, `cargo-llvm-cov`, `cargo-cyclonedx`, and `cosign` are on your `$PATH` for local pre-flight checks.
- A repository Actions secret named `CARGO_REGISTRY_TOKEN` is configured for crates.io publishing. The release workflow skips crates.io publishing when this secret is absent.

This repository has split Cargo workspaces:

- `core/Cargo.toml` for foundation crates and the `rskit` facade.
- `contrib/Cargo.toml` for adapter crates.
- `examples/Cargo.toml` for demos; examples are validated but not published.

There is intentionally no root `Cargo.toml`.

## 1. Decide the version

```sh
# What's the latest tag?
git tag --sort=-v:refname | head -1

# What changed since then?
git log --oneline $(git describe --tags --abbrev=0)..HEAD
```

Use the [SEMVER policy](./policy/SEMVER.md) to pick the next version. While in `0.x`, every release with a breaking change in the `[Unreleased]` CHANGELOG section bumps MINOR; otherwise PATCH.

## 2. Update the CHANGELOG

1. Open `CHANGELOG.md`.
2. Replace `## [Unreleased]` with `## [vX.Y.Z] - YYYY-MM-DD`.
3. Add a fresh empty `## [Unreleased]` section above it.
4. If `[Unreleased]` is empty, refuse to release — there is nothing to ship.
5. Update the link reference at the bottom of the file (if present).

CI refuses to tag if `[Unreleased]` is the only populated section, or if `[vX.Y.Z]` for the version you're cutting doesn't exist in the file.

## 3. Bump versions across the workspaces

All publishable crates currently share a single lock-step version. Release preparation is manual for the first public release because the repository has split workspaces and no root `Cargo.toml`.

When preparing a release PR, update both split workspace manifests:

- `core/Cargo.toml`: `[workspace.package].version`.
- `contrib/Cargo.toml`: `[workspace.package].version`.
- Internal dependency versions in `contrib/Cargo.toml` that point at core crates.
- Any internal dependency versions in crate manifests that do not inherit from the workspace dependency tables.

Then refresh the split lockfiles:

```sh
cargo update --manifest-path core/Cargo.toml --workspace
cargo update --manifest-path contrib/Cargo.toml --workspace
cargo update --manifest-path examples/Cargo.toml --workspace
git add core/Cargo.toml contrib/Cargo.toml core/Cargo.lock contrib/Cargo.lock examples/Cargo.lock CHANGELOG.md
git commit -S -m "chore: prepare vX.Y.Z release"
```

## 4. Pre-flight checks

```sh
make check
make deny
make release-readiness
make release-coverage
make release-sbom
make publish-dry-run
```

If any check fails, fix it before publishing the GitHub Release.

### First-release publish rehearsal

`cargo publish --dry-run` resolves registry dependencies from crates.io; it does not simulate publishing an unpublished internal dependency chain. For a lock-step first release, or any release where internal crates depend on the same not-yet-published version, `make publish-dry-run` therefore:

1. Runs `cargo publish --dry-run --locked` for crates whose internal same-version dependencies already exist on crates.io.
2. Explicitly skips crates blocked by unpublished internal same-version dependencies and runs `cargo package --locked --list` as a packaging sanity check for each skipped crate.
3. Prints a notice listing the skipped crates, so the rehearsal does not claim full crates.io dependency-chain validation.

The reliable first-release gate is the combination of full workspace build/test/docs/audit/coverage, generated publish order, package-list sanity checks for blocked crates, and the GitHub Release workflow publishing crates in dependency order. If any real publish step fails, stop and fix forward before continuing the chain.

## 5. Publish the GitHub Release

Use the GitHub Release UI as the publishing entrypoint:

1. Open <https://github.com/kbukum/rskit/releases/new>.
2. Set **Choose a tag** to `vX.Y.Z`, then choose **Create new tag: vX.Y.Z on publish**.
3. Set **Target** to `main`.
4. Use **Generate release notes**.
5. For pre-releases such as `v0.1.0-alpha.1`, check **Set as a pre-release**.
6. Publish the release.

Publishing the GitHub Release creates the `v*` tag and triggers `.github/workflows/release.yml` from the `release.published` event. The workflow verifies that the tag starts with `v` and points at `main`, then runs the release gates, publishes crates, signs SBOMs, and uploads generated assets back to the same GitHub Release.

Directly pushing a `v*` tag does not trigger publishing. The release workflow (`.github/workflows/release.yml`) is triggered by publishing a GitHub Release and will:

- Re-run the full test + lint + audit suite on the tagged commit.
- Generate release coverage reports and enforce release coverage thresholds.
- Dry-run publishing where same-version internal dependencies already exist on crates.io, package-list crates blocked by unpublished internal dependencies, then publish every publishable workspace crate to crates.io in dependency order when `CARGO_REGISTRY_TOKEN` is configured.
- Generate and attach a CycloneDX SBOM (`cargo-cyclonedx`).
- Sign the release SBOMs with [cosign](https://github.com/sigstore/cosign).
- Upload generated assets to the GitHub Release that triggered the workflow.

## 6. Verify the workflow release

Watch the release workflow until it completes:

```sh
RUN_ID=$(gh run list --workflow Release --limit 1 --json databaseId --jq '.[0].databaseId')
gh run view "$RUN_ID" --log-failed
```

If publishing fails after some crates were uploaded, do not delete or force-push the tag. Fix forward with a new version because crates.io versions are immutable.

## 7. Verify on crates.io and docs.rs

```sh
cargo search rskit
# Check https://crates.io/crates/rskit-toolkit/X.Y.Z
# Check https://docs.rs/rskit-toolkit/X.Y.Z
```

If `docs.rs` fails to build, investigate the build log on `https://docs.rs/crate/rskit-toolkit/X.Y.Z/builds`.

## 8. Announce

- Post in the project's discussion / README "Latest" section.
- Open a "post-release smoke test" issue against the next sprint milestone.
- Notify sibling repos ([`gokit`](https://github.com/kbukum/gokit), [`pykit`](https://github.com/kbukum/pykit)) if any cross-sibling APIs changed.

## Hotfix releases

Hotfixes follow the same GitHub Release flow but skip the `[Unreleased]` rotation if the fix is targeted at an older line. Prepare the hotfix commit, add a `## [vX.Y.Z] - YYYY-MM-DD` section to `CHANGELOG.md`, merge the hotfix line, then publish the GitHub Release from the Releases UI.

## Pre-releases

For the first public crates.io publish, prefer `v0.1.0-alpha.1` so downstream users can opt into an explicitly cautious preview before a final `v0.1.0`. Follow the same release flow above and check **Set as a pre-release** in the GitHub Release UI before publishing.
