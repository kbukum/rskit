---
name: new-crate
description: >-
    Scaffold a new crate in the rskit multi-workspace monorepo the canonical way — decide
    core vs contrib, wire the workspace Cargo.toml, inherit workspace metadata, add
    #![warn(missing_docs)], and wire the facade. Use when adding a new capability, foundation
    crate, or adapter to rskit, or when unsure whether new code belongs in core or contrib.
---

# Adding a crate to rskit

rskit splits its Cargo workspaces by role: foundation crates and the `rskit` facade live under
`core/`, adapter crates under `contrib/<domain>/`, and demos under `examples/`. There is
intentionally **no root `Cargo.toml`**. Getting placement and wiring right up front avoids
layering violations and facade drift later.

## Step 1 — Decide: core, contrib, or example

- **Shared foundation / cross-cutting capability** (errors, config, logging, provider, pipeline,
  resilience, di, auth, observability, …) → `core/rskit-<name>/`.
- **Provider/adapter for an external system** (a cloud SDK, driver, broker client, ML runtime) →
  `contrib/<domain>/<name>/` (`storage`, `cache`, `messaging`, `inference`, `llm`, `media`,
  `vectorstore`). See the `new-backend` skill for the adapter specifics.
- **Demo / sample app** → `examples/<name>/` (validated, never published).

When in doubt between core and contrib, ask: does it pull a heavy external dependency? Heavy dep →
contrib; stdlib + rskit crates only → core.

## Step 2 — Confirm dependency direction

rskit layers depend **downward only** — a lower crate never depends on a higher one; the `rskit`
facade sits at the top and only re-exports. A lower layer importing a higher one, or behavior
added directly to the facade, is a **blocker**. `make check-topology` guards placement and
acyclicity.

## Step 3 — Create the crate

```bash
cargo new --lib core/rskit-<name>        # or contrib/<domain>/<name>
```

In `Cargo.toml`, inherit workspace package metadata (`version.workspace = true`, etc.) and rely on
the workspace lints. Every crate carries crate-level docs and:

```rust
#![warn(missing_docs)]
//! <one-line crate responsibility>.
//!
//! <2–3 lines on the model, invariants, and what it deliberately does not do>.
```

Conventions from `.github/copilot-instructions.md`: typed, minimal public API (no broad `Any`);
`#[must_use]` on `with_*` builders; `#[non_exhaustive]` on public enums that may grow; no
`unwrap()`/`expect()` in library code; `AppError`/`AppResult` for errors; `parking_lot::Mutex` over
`std::sync::Mutex`; no `unsafe` without a `// SAFETY:` comment. Organize by focused, concern-named
files (types, options, registry, adapter) from the start — `lib.rs`/`mod.rs` stay declare-only
(submodule declarations + re-exports, no logic), never a monolithic starter file. Before adding a
shared helper, check [`docs/CONCERN-OWNERS.md`](../../../docs/CONCERN-OWNERS.md) so the new crate
does not re-own an existing concern.

## Step 4 — Wire the workspace and facade

- Add the crate to the matching workspace: `core/Cargo.toml` members, or the
  `contrib/Cargo.toml` / `examples/Cargo.toml` member pattern.
- For a core capability that consumers should reach through the facade, re-export it from the
  `rskit` facade (adapter integrations behind a **feature flag**, not unconditionally).
- Keep shared dependency versions consistent across workspaces
  (`make check-workspace-deps-sync`).

## Step 5 — Parity awareness

rskit is the **reference** kit that gokit/pykit mirror. If this capability should be tracked
cross-kit, note it for the sibling repos (see the `sibling-parity` skill) rather than assuming it
stays rskit-only.

## Step 6 — Validate

```bash
make fmt
make build C=rskit-<name>
make lint  C=rskit-<name>
make test  C=rskit-<name>
make check-topology
make doc   C=rskit-<name>
```

## Checklist

- [ ] Placement decided (core / contrib / examples) and justified by real deps
- [ ] Dependency direction downward-only (`check-topology` clean); facade only re-exports
- [ ] `#![warn(missing_docs)]` + crate docs; files split by concern; `lib.rs`/`mod.rs` declare-only
- [ ] Public API typed/minimal, builders `#[must_use]`, growable enums `#[non_exhaustive]`
- [ ] Added to the right workspace `Cargo.toml`; facade wired (feature-gated if an adapter)
- [ ] Shared dep versions consistent (`check-workspace-deps-sync`)
- [ ] build/lint/test/doc green for the crate

Per repo workflow, **create the branch and make edits only** — the maintainer commits and pushes.
