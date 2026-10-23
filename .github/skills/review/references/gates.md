# Lens: gates and enforcement

Separate rules a gate checks from rules that only reviewers check. Green gates are necessary, not proof.

Rules: [validation](../../../engineering.md#validation), [known gaps](../../../engineering.md#known-gaps). The gate list itself is `make help`, `toven.toml`, and the [CI workflow](../../../workflows/ci.yml).

## What to look for

- **Gate ran.** The scoped gates for the change passed, and the result is fresh.
- **Gate covers the crate.** A crate without `[lints] workspace = true` skips the workspace lints; say so instead of trusting `make lint`.
- **Claims match gates.** Docs, skills, and PR text do not call a review-only rule enforced.
- **Review-only rules.** Check the [known gaps](../../../engineering.md#known-gaps) by hand: `#[non_exhaustive]`, `#[must_use]`, test-first, semver breaks, race and shuffle runs.
- **Gate changes.** A weakened or removed gate needs a reason and an updated known-gaps entry.

## Severity

| Severity | Examples |
|---|---|
| Blocker | Required gate failing or skipped; gate weakened to pass |
| Should-fix | False enforcement claim; new rule with no gate and no known-gap entry |
| Nit | Gate run broader than needed |

## Detection starters

```bash
make help
toven groups
git diff <base>...HEAD -- Makefile toven.toml .github/workflows scripts
```
