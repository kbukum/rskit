# Naming and structure

The single source of truth for **naming** and **file/module organization** of rskit source, capturing rules the engineering baseline already applies so they can be checked consistently. It is the companion to [`CONCERN-OWNERS.md`](CONCERN-OWNERS.md) (which owns *where a concern lives*) and [`TEST-LAYOUT.md`](TEST-LAYOUT.md) (which owns *where tests live*). The authoritative baseline is [`.github/copilot-instructions.md`](../.github/copilot-instructions.md); this doc collects the naming/organization slice of it in one referenceable place, and is the Rust sibling of gokit's `docs/naming-and-structure.md`.

## Naming

- **Crate and module names** are non-stuttering — a module inside `rskit-agent` is not also `agent` such that the path reads `rskit_agent::agent::…`; prefer the crate-root re-export as the call-site API. A misleading or stuttering name is a defect: rename it and migrate callers in the same change.
- **Public items read correctly at the call site.** Prefer `storage::Registry` over `storage::store::StorageRegistry`; drop the module/crate name from the identifier when the qualified path already carries it.
- **Files are named after the concern they hold** — `client.rs`, `registry.rs`, `channel.rs`, `error.rs`. Generic names (`common.rs`, `types.rs`, `util.rs`) are a smell when they obscure a specific owned concern (e.g. a `types.rs` that actually owns the streaming protocol should be `stream.rs`); acceptable only when the file's concern genuinely is that.
- **Test-helper naming** in `rskit-testutil` and domain testutil modules is consistent for equivalent shapes: lifecycle/behavior doubles as `Fake<Thing>`, stateful configurable mocks as `Mock<Thing>`, resource-owning harnesses as `Test<Thing>`, fluent setup as `with_*`. An outlier form for the same shape (a second name for the same concept, e.g. `RepoBuilder` beside `TestRepo`) is reconciled to one name and callers migrated.

## File and module organization

Organize by focused, well-named files within a crate; never pile unrelated concerns into one file.

- **One concern per file.** Split a crate's code by concern into concern-named sibling files.
- **Aggregators are declare-only.** `lib.rs`/`mod.rs` carry only crate-root attributes, module declarations, and re-exports — no logic, no private items, no inline `#[cfg(test)] mod tests`. Enforced by `scripts/sg-rules/declare-only-aggregator.yml` via `make structure`.
- **Every crate carries `#![warn(missing_docs)]`** on its `lib.rs`.
- **Oversized-file signal.** A single non-test `.rs` file past roughly **300–400 lines of real code** (code only — excluding `#[cfg(test)]`/`#[test]` code, comments, blanks) is a prompt to check whether distinct concerns are piled together. Length alone is never the verdict: a cohesive single-concern file is fine at any size; **concern-mixing** is the real signal. When a change touches such a file, promote it to a folder (declare-only `mod.rs` + concern-named submodules, `#[cfg(test)] test_support` for shared fixtures) in the same change rather than deferring.
- **Sub-module lift.** When a single module/crate accumulates **more than ~10 non-test files** (excluding `test_support`/`tests`; `make structure` warns at `CROWDED_MODULE_FILES=15`) that fall into **2–3+ separable, loosely-coupled concern groups**, lift each cohesive group into its own concern-named submodule folder (nested `mod.rs`) — the `http/`, `discovery/`, `apikey/` shape. Criteria-driven, not a file count: only split where the groups are genuinely separable and it improves discoverability without causing other issues (cycles, over-fragmentation).
- **Placement and layering** (which crate/workspace/layer code belongs in, acyclic downward dependency direction) are covered by [`CONCERN-OWNERS.md`](CONCERN-OWNERS.md) and review pass `00`; they are not restated here.

## Enforcement

The declare-only aggregator gate is **enforced** (`make structure`); the oversized-file and crowded-module triggers are **advisory** (reviewer judgment — acceptance is "every remaining warning is a deliberate, recorded judgment", not "zero warnings"). Naming and file-organization judgments are carried by review pass `00` (`.github/skills/review/references/00-structure-placement.md`). A layer-hardening step fixes the violations attributed to its scope; this convention is the rule source it fixes against.
