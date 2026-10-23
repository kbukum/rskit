# Lens: placement and ownership

Check first. Misplaced or duplicated code makes every later lens unreliable, so a blocker here stops acceptance.

Rules: [placement and ownership](../../../engineering.md#placement-and-ownership), [concern owners](../../../../docs/CONCERN-OWNERS.md), [naming and structure](../../../../docs/naming-and-structure.md), [test layout](../../../../docs/TEST-LAYOUT.md).

## What to look for

- **Layer.** Code lives in the lowest layer that owns the concern. Foundations in `core/rskit-<name>/`, adapters in `contrib/<domain>/<name>/`, demos in `examples/`. Imports point down only; no cycles.
- **New crate wiring.** Own `version`, workspace metadata, `[lints] workspace = true`, listed in its `toven.toml` group, regenerated module index, facade wiring behind a feature when it belongs there.
- **Facade.** `rskit-suite` re-exports; it holds no logic.
- **Owner reuse.** Name the concern and find its owner. A new error enum, hand-rolled retry, raw `reqwest`, `Command::new`, second logger, or near-copy of an owner is duplication. If the owner falls short, enhance it generically.
- **Keep code current.** The standard library or an existing owner may already cover a new helper or dependency.
- **Module structure.** Declare-only `lib.rs` / `mod.rs`. Split concern-mixed files and crowded modules by responsibility, not size.
- **Naming.** Non-stuttering paths, concern-named files, one name per test-double shape.
- **Test placement.** Three tiers from the test layout; shared doubles in `rskit-testutil`.

## Severity

| Severity | Examples |
|---|---|
| Blocker | Upward import or cycle; adapter in `core/`; logic duplicating an owner |
| Should-fix | Missing crate wiring or lint inheritance; logic in an aggregator or facade; concern-mixed file left unsplit; stuttering name; inline fake that belongs in `rskit-testutil` |
| Nit | File naming that is clear but could be sharper |

## Detection starters

Hits are candidates, not verdicts.

```bash
rg 'use rskit::' core/rskit-*/src contrib/*/*/src          # facade imported from below
rg -n '^rskit-' core/rskit-*/Cargo.toml contrib/*/*/Cargo.toml   # internal edges per crate
rg --files-without-match '^\[lints\]' core/rskit-*/Cargo.toml contrib/*/*/Cargo.toml
rg 'thiserror|impl .*Error for' core/*/src contrib/*/*/src    # parallel error types
rg 'Command::new|reqwest::Client|hyper::Client' core/*/src contrib/*/*/src
rg 'retry|backoff' core/*/src contrib/*/*/src                  # hand-rolled resilience
toven groups                                                   # layer membership
make structure check-topology check-l7-edges
```
