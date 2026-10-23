---
name: release
description: "rskit: Prepare or publish a release through the repository's version, validation, and supply-chain gates."
---

# Release

Follow [releasing](../../../docs/RELEASING.md), [versioning](../../../docs/VERSIONING.md), and the [engineering standard](../../engineering.md). Split Cargo workspaces publish independently versioned crates in dependency order. Toven stages bumps; the maintainer owns the release commit and signed umbrella tag.

1. Inspect clean `main`, tags, current manifests, release tooling, credentials availability, and requested scope. Use `make setup` and `scripts/setup.sh --release` only when needed.
2. Run full `make check`, `make release-readiness`, `make release-coverage`, and a project review. Preserve configured coverage and supply-chain gates.
3. Inspect `make release-plan` and `make release-status`. A pre-1.0 break requires a minor bump; otherwise patch.
4. Run the canonical bump on clean `main` **before** changelog edits; branch-channel selection maps `main` to `alpha`. Then rotate concise changelog notes and carry the staged bump, dependency floors, and README pins to a release branch.
5. Land preparation through a reviewed PR only when authorized. Never push a bump commit directly to protected `main`.
6. After merge, run SBOM and publish rehearsal; the authorized maintainer creates, verifies, and pushes the signed umbrella tag. `entrypoint = "maintainer"` means Toven verifies it rather than creating or moving it.
7. Publish through the documented workflow or authorized local command. Verify every crate, tag, hosted asset, SBOM, and signature; skipped publication is not success.

Preserve user work. Commits, pushes, PRs, tagging, and publication need authorization. Never move published tags or retain credentials; use full URLs for cross-repository references.
