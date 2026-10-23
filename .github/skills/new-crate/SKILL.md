---
name: new-crate
description: "rskit: Add a Rust crate in the correct layer with workspace wiring, lints, docs, and shared tests."
---

# Adding a crate to rskit

Rules: [placement and ownership](../../engineering.md#placement-and-ownership), [public API](../../engineering.md#public-api-and-stability), [code style](../../engineering.md#code-style). Contributor steps: [adding a new crate](../../../CONTRIBUTING.md#adding-a-new-crate).

## Steps

1. **Check the owner.** Find the concern in [concern owners](../../../docs/CONCERN-OWNERS.md). If an owner exists, enhance it instead of adding a crate.
2. **Pick the place.** Shared foundation with only std and rskit deps goes in `core/rskit-<name>/`. An adapter for an external system goes in `contrib/<domain>/<name>/` (see `new-backend`). A demo goes in `examples/<name>/`.
3. **Pick the layer.** Choose the lowest `[groups.*]` in `toven.toml` whose `guardrails.allow_groups` covers the crate's dependencies.
4. **Create it.** `cargo new --lib core/rskit-<name>` (or the contrib path). Give it its own `version`, inherit the other workspace metadata, and add `[lints] workspace = true`. Write a `//!` crate doc with its purpose and place in the toolkit. Keep `lib.rs` declare-only and split files by concern from the start.
5. **Wire it.** Add it to the workspace members, to its group's `modules` in `toven.toml`, and run `toven run module-index`. Re-export it from the `rskit-suite` facade when it belongs there; adapters go behind a feature flag.
6. **Parity.** If gokit has or needs the same contract, follow the `sibling-parity` skill.

## Validate

```bash
make fmt C=rskit-<name>
make lint C=rskit-<name>
make test C=rskit-<name>
make doc C=rskit-<name>
make check-workspace-deps-sync
toven groups                  # the crate is listed under its layer
```

## Acceptance

- [ ] No existing owner covered the concern.
- [ ] Right workspace and layer; Toven planning accepts its dependencies.
- [ ] Own version, workspace metadata, workspace lints, crate docs, declare-only `lib.rs`.
- [ ] Listed in `toven.toml`, module index regenerated, facade wired when needed.
- [ ] Format, lint, test, and doc gates pass for the crate.

Leave edits uncommitted. Git actions need authorization.
