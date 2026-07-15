# Pass 00 — Structure and placement

Confirm every touched (or, in project mode, every existing) item lives in the right workspace, crate, and layer, and that the dependency direction stays acyclic. This is the first gate: misplaced code makes every later pass moot, so reject on failure here before going further.

> **Run in a separate, clean-context agent** — never inline in the session that wrote the code. An independent reviewer re-derives every judgment from the code and the principles instead of trusting prior reasoning. A plan/spec may be passed in as a scope checklist only; it never excuses a baseline violation.

**Scope note.** *Changes mode:* check the crates the diff touches plus the blast radius — a change to a core crate's public surface fans out to the facade, other core crates, every `contrib/` adapter, and downstream consumers. *Project mode:* sweep each workspace's members and dependency edges; the placement and acyclicity rules below are invariants for the whole toolkit.

## The layering invariant

Dependency direction is explicit and acyclic; lower layers never import higher. A cycle or an upward import is a **blocker**. The workspaces are split by role:

| Workspace | Path | Owns |
|-----------|------|------|
| core | `core/rskit-<name>/` | foundation + cross-cutting crates, and the `rskit` facade |
| contrib | `contrib/<domain>/<name>/` | domain adapters (`storage`, `cache`, `messaging`, `inference`, `llm`, `media`, `vectorstore`) |
| examples | `examples/<name>/` | demos and sample applications |

## Checks

- **Crate placement.** Foundation/cross-cutting code → `core/rskit-<name>/`. Domain adapter → `contrib/<domain>/<name>/`. Demo → `examples/<name>/`. A foundation concern living under `contrib/`, or an adapter under `core/`, is a structure violation (blocker).
- **Acyclic, downward-only edges.** No core crate imports the facade; no lower crate imports a higher one; no cycle between crates. This is gated by `make check-topology` and `make check-l7-edges` — run them.
- **New core crate wiring.** Created under `core/rskit-<name>/`, added to `core/Cargo.toml`, inherits workspace package metadata, carries `#![warn(missing_docs)]`, and is wired into the `rskit` facade as appropriate. Missing any of these is a should-fix.
- **New adapter wiring.** Under `contrib/<domain>/<name>/`, covered by the matching `contrib/Cargo.toml` member pattern, and exposed through the facade **behind a feature flag** — not unconditionally.
- **Facade discipline.** The `rskit` facade re-exports; it does not contain logic. Behavior added directly to the facade is misplaced (should-fix).
- **No misplaced concerns.** Each cross-cutting concern stays in its canonical crate — e.g. gRPC status mapping belongs in `rskit-grpc`, not `rskit-errors`. (Reuse of those owners is pass `01`.)
- **Workspace dep sync.** Shared dependency versions stay consistent across `core`/`contrib`; gated by `make check-workspace-deps-sync`.

## Detection starters

These flag candidates, not verdicts — read each hit to judge intent.

```bash
# facade or higher-layer imports inside a core crate
rg 'use rskit::' core/rskit-*/src
# adapter reaching into another adapter or the facade
rg 'use rskit::' contrib/*/*/src
# what each crate actually depends on
for c in core/rskit-*/Cargo.toml contrib/*/*/Cargo.toml; do echo "== $c =="; rg '^rskit-' "$c"; done
```

Then run `make check-topology check-l7-edges check-workspace-deps-sync` for the placement/acyclicity guards.
