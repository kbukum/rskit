# Lens: dependencies and supply chain

rskit ships to downstream users, so every dependency and CI change is part of their supply chain.

Rules: [dependencies and supply chain](../../../engineering.md#dependencies-and-supply-chain), [SECURITY](../../../../SECURITY.md#supply-chain), [releasing](../../../../docs/RELEASING.md).

## What to look for

- **New dependency.** Maintained, allowed license, no advisory, not duplicating std or an rskit owner. Added through the owning workspace manifest.
- **Lockfiles.** Each affected workspace `Cargo.lock` updated and committed with the manifest change.
- **Shared versions.** In sync across `core`, `contrib`, and `examples`.
- **CI actions.** Pinned to a full commit SHA.
- **Release tooling.** Signed artifacts and SBOMs still produced.

## Severity

| Severity | Examples |
|---|---|
| Blocker | Dependency with an open advisory or disallowed license; unpinned action; lockfile out of sync |
| Should-fix | Unjustified dependency; duplicated version across workspaces |
| Nit | Feature flags on a dependency wider than needed |

## Detection starters

```bash
git diff <base>...HEAD -- '**/Cargo.toml' '**/Cargo.lock'
rg -n --pcre2 'uses:\s+[^\s#]+@(?![0-9a-f]{40}\b)' .github/workflows
make check-workspace-deps-sync
make deny
```
