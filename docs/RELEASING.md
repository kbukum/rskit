# Releasing

The mechanical steps to cut a release of `rskit`. For *what* counts as a
breaking change vs a feature vs a fix, see [`policy/SEMVER.md`](policy/SEMVER.md)
and [`policy/DEPRECATION.md`](policy/DEPRECATION.md).

## Prerequisites

- You are listed in `MAINTAINERS.md` and have push access to `kbukum/rskit`.
- Your local clone is on `main` with no uncommitted changes.
- `git`, `gh`, `cargo`, `cargo-nextest`, `cargo-deny`, `cargo-audit`,
  `cargo-llvm-cov`, `cargo-cyclonedx`, and `cosign` are on your `$PATH`.
- Your commits are GPG-signed (`git config commit.gpgsign true`) — release
  tags must be signed.
- A `CARGO_REGISTRY_TOKEN` is configured in CI for crates.io publishing
  (preferably via Trusted Publishing once available).

## 1. Decide the version

```sh
# What's the latest tag?
git tag --sort=-v:refname | head -1

# What changed since then?
git log --oneline $(git describe --tags --abbrev=0)..HEAD
```

Use the [SEMVER policy](./policy/SEMVER.md) to pick the next version. While
in `0.x`, every release with a breaking change in the `[Unreleased]`
CHANGELOG section bumps MINOR; otherwise PATCH.

## 2. Update the CHANGELOG

1. Open `CHANGELOG.md`.
2. Replace `## [Unreleased]` with `## [vX.Y.Z] - YYYY-MM-DD`.
3. Add a fresh empty `## [Unreleased]` section above it.
4. If `[Unreleased]` is empty, refuse to release — there is nothing to ship.
5. Update the link reference at the bottom of the file (if present).

CI refuses to tag if `[Unreleased]` is the only populated section, or if
`[vX.Y.Z]` for the version you're cutting doesn't exist in the file.

## 3. Bump versions across the workspace

All crates in rskit currently share a single workspace version (lock-step).
Bump it once via the workspace inheritance:

```sh
# Edit Cargo.toml: [workspace.package] version = "X.Y.Z"
# All crates inherit via `version.workspace = true`.
cargo set-version --workspace X.Y.Z   # cargo-edit
```

Then refresh the lockfile:

```sh
cargo update --workspace
git add Cargo.toml Cargo.lock CHANGELOG.md
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

If any check fails, fix it before tagging.

### First-release publish rehearsal

`cargo publish --dry-run` resolves registry dependencies from crates.io; it
does not simulate publishing an unpublished internal dependency chain. For a
lock-step first release (or any release where internal crates depend on the
same not-yet-published version), `make publish-dry-run` therefore:

1. Runs `cargo publish --dry-run --locked` for crates whose internal
   same-version dependencies already exist on crates.io.
2. Explicitly skips crates blocked by unpublished internal same-version
   dependencies and runs `cargo package --locked --list` as a packaging sanity
   check for each skipped crate.
3. Prints a notice listing the skipped crates, so the rehearsal does not claim
   full crates.io dependency-chain validation.

The reliable first-release gate is the combination of full workspace
build/test/docs/audit/coverage, generated publish order, package-list sanity
checks for blocked crates, and the actual tag workflow publishing crates in
dependency order. If any real publish step fails, stop and fix forward before
continuing the chain.

## 5. Tag the release

```sh
git tag -s -a vX.Y.Z -m "vX.Y.Z"
git push origin main vX.Y.Z
```

The release workflow (`.github/workflows/release.yml`) is triggered by the
tag push and will:

- Re-run the full test + lint + audit suite on the tagged commit.
- Dry-run publishing where same-version internal dependencies already exist on
  crates.io, package-list crates blocked by unpublished internal dependencies,
  then publish every publishable workspace crate to crates.io in dependency
  order when `CARGO_REGISTRY_TOKEN` is configured.
- Sign the release SBOMs with [cosign](https://github.com/sigstore/cosign).
- Generate and attach a CycloneDX SBOM (`cargo-cyclonedx`).

## 6. Cut the GitHub Release

Once the workflow completes successfully, generate release notes from the
CHANGELOG and create the GitHub Release via `gh`:

```sh
./scripts/release-notes.sh vX.Y.Z > /tmp/notes.md
gh release create vX.Y.Z --title "vX.Y.Z" --notes-file /tmp/notes.md \
  --verify-tag
```

Attach the cosign signature bundle and SBOM to the Release as assets if the
workflow did not already.

## 7. Verify on crates.io and docs.rs

```sh
cargo search rskit
# Check https://crates.io/crates/rskit/X.Y.Z
# Check https://docs.rs/rskit/X.Y.Z
```

If `docs.rs` fails to build, investigate the build log on
`https://docs.rs/crate/rskit/X.Y.Z/builds`.

## 8. Announce

- Post in the project's discussion / README "Latest" section.
- Open a "post-release smoke test" issue against the next sprint milestone.
- Notify sibling repos ([`gokit`](https://github.com/kbukum/gokit),
  [`pykit`](https://github.com/kbukum/pykit)) if any cross-sibling APIs
  changed.

## Hotfix releases

Hotfixes follow the same flow but skip the `[Unreleased]` rotation if the
fix is targeted at an older line:

```sh
git checkout v0.2.0
git checkout -b hotfix/v0.2.1
# … apply fix …
# add a `## [0.2.1] - YYYY-MM-DD` section to CHANGELOG.md
git tag -s -a v0.2.1 -m "v0.2.1"
git push origin v0.2.1
```

## Pre-releases

For the first public crates.io publish, prefer `v0.1.0-alpha.1` so downstream
users can opt into an explicitly cautious preview before a final `v0.1.0`.

```sh
git tag -s -a v0.1.0-alpha.1 -m "v0.1.0-alpha.1"
git push origin v0.1.0-alpha.1
gh release create v0.1.0-alpha.1 --prerelease --title "v0.1.0-alpha.1" \
  --notes-file /tmp/notes.md
```

Pre-releases bypass the CHANGELOG check (the `-alpha.N`, `-beta.N`, or
`-rc.N` suffix is detected by the release workflow).
