## Description

<!-- Provide a clear and concise description of your changes -->

## Motivation

<!-- Why is this change needed? What problem does it solve? Link to related issues: Fixes #123 or Closes #456 -->

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

- - -

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

<!-- rskit and gokit share concepts and wire contracts, not internal shapes. If this change touches a shared contract (error codes, Component lifecycle, Provider, Pipeline, wire formats), confirm parity or link the sibling issue. -->

- [ ] Sibling-parity not required (internal change)
- [ ] Sibling-parity tracked: gokit#___

## Checklist

<!-- Rules: .github/engineering.md. See its Definition of Done. -->

- [ ] Code sits in the right layer and reuses the concern owner
- [ ] Trust boundaries, bounds, cancellation, and shutdown are handled; no runtime `unwrap` / `expect` / `panic!`
- [ ] Public API is minimal and typed; public items have rustdoc
- [ ] Tests cover the change, including failure paths, and are deterministic
- [ ] New dependencies are justified; affected lockfiles are updated and committed
- [ ] CHANGELOG entry added under `[Unreleased]`; breaking changes marked **Breaking**

## Additional Notes

<!-- Any extra context, screenshots, or information for reviewers -->
