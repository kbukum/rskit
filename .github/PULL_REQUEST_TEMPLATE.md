## Description

<!-- Provide a clear and concise description of your changes -->

## Motivation

<!-- Why is this change needed? What problem does it solve? --> <!-- Link to related issues: Fixes #123 or Closes #456 -->

## Type of Change

<!-- Mark the relevant option with an 'x' -->

- [ ] Bug fix (non-breaking change which fixes an issue)
- [ ] New feature (non-breaking change which adds functionality)
- [ ] Breaking change (fix or feature that would cause existing functionality to not work as expected)
- [ ] Documentation update
- [ ] Refactoring (no functional changes)
- [ ] Performance improvement
- [ ] Test coverage improvement

## Crate(s) Affected

<!-- List the crates this PR changes (e.g., rskit-errors, rskit-server) -->

-

## Changes Made

<!-- List key changes in bullet points -->

-
-
-

## Testing

<!-- Describe how you tested your changes -->

- [ ] `make test` or an affected-crate equivalent passes locally
- [ ] `make lint` or an affected-crate equivalent is clean
- [ ] `make fmt-check` passes
- [ ] `make deny` or the affected workspace policy check passes
- [ ] `make doc` or an affected-crate equivalent builds without warnings
- [ ] Manual testing performed (describe below if applicable)

### Test Evidence

<!-- Optional: show test output, screenshots, or logs demonstrating your changes work -->

```
$ make test C=rskit-<name>
...
```

## Breaking Changes

<!-- If this is a breaking change, describe the impact and migration path -->

## Sibling Parity

<!-- rskit mirrors gokit and pykit. If this change touches a public abstraction
(error codes, Component lifecycle, Provider, Pipeline, etc.), confirm parity
or link the corresponding sibling issue. -->

- [ ] Sibling-parity not required (internal change)
- [ ] Sibling-parity tracked: gokit#___, pykit#___

## Checklist

- [ ] Public API items have `///` doc comments
- [ ] New `with_*` builder methods are annotated `#[must_use]`
- [ ] New public enums are `#[non_exhaustive]` if they may grow
- [ ] No `unwrap()` / `expect()` in library code (tests are fine)
- [ ] Any new `unsafe` block has a `// SAFETY:` comment
- [ ] Affected split workspace lockfiles updated and committed if dependencies changed
- [ ] CHANGELOG entry added under `[Unreleased]`
- [ ] New dependencies (if any) are justified and pass the relevant `make deny` check

## Additional Notes

<!-- Any extra context, screenshots, or information for reviewers -->
