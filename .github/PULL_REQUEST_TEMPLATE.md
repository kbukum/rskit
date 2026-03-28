## Summary

<!-- What does this PR do? 1-3 bullet points. -->

-
-

## Related issue(s)

<!-- Closes #<issue>, Fixes #<issue> -->

## Checklist

- [ ] `cargo test --workspace` passes locally
- [ ] `cargo clippy --workspace -- -D warnings` is clean
- [ ] `cargo fmt --check` passes
- [ ] Public API items have `///` doc comments
- [ ] New `with_*` builder methods are annotated `#[must_use]`
- [ ] New public enums are `#[non_exhaustive]` if they may grow
- [ ] `CHANGELOG.md` updated under `## [Unreleased]`
- [ ] No `unwrap()` / `expect()` in library code (tests are fine)

## Test plan

<!-- How was this tested? List new or existing tests that cover the change. -->

## Breaking changes

<!-- Does this change any public API? If yes, describe what is affected and why the break is necessary. -->
