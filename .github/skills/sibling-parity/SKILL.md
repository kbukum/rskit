---
name: sibling-parity
description: >-
    Keep rskit as the reference kit that gokit (Go) and pykit (Python) mirror — judge cross-kit
    parity by capability, keep docs/parity-matrix.md accurate, propagate capability and quality
    changes down to the sibling kits, and improve rskit generically (never consumer-specific). Use
    when touching anything with a cross-kit parity row or aligning a capability across kits.
---

# Keeping rskit the reference for its sibling kits

rskit is a sibling kit to gokit (Go) and pykit (Python): same module structure and naming,
same engineering baseline, idiomatic per language. **rskit received a full quality pass
and is the current reference shape/quality** — the sibling kits are brought up to rskit's level,
not the reverse. This skill keeps that relationship honest as rskit changes.
The siblings live in `../gokit` and `../pykit`.

## Parity is judged by capability, not blindly

Cross-kit parity weighs where each language is strongest —
it is **not** a demand that every rskit symbol appear in every kit. Decide per capability:

- **Shared infrastructure** (errors, config, di, provider shapes, resilience, transport, data adapters) is expected to exist in every kit;
  keep the abstraction and naming aligned.
- **Rust-strongest capabilities stay rskit-only
  or "light" elsewhere.** Heavy media/video/audio/ matrix work belongs in Rust;
  gokit/pykit `media` are deliberately **light** (detection, metadata, cheap image ops, time/spatial types, subtitles).
  Record these as intentional rows, not gaps.
- **Framework-specific concepts** (e.g. rskit `http` is Axum-specific and folds into gokit `server`; gokit `connect` is ConnectRPC-specific with no rskit peer) are deliberate divergences with a note,
  not gaps to close.

## Workflow when rskit changes a shared capability

1. **Locate the counterpart.** Find the sibling crate/package/module in `../gokit` (and `../pykit`)
   and its public API, invariants, and error model.
2. **Decide the mirroring level** (full / light / kit-only) using the rules above.
3. **Propagate downward.** rskit is the source of truth: when a shared abstraction changes here,
   flag the sibling change needed (an issue/note in the sibling repo, referenced by **full URL**),
   or align it if that is in scope. Do not silently let the kits drift.
4. **Update `docs/parity-matrix.md`.** Adjust the module-presence row (✅ present · ➖ absent · ⏳ planned)
   and any capability tables. The module-presence table is a shared cross-kit source —
   keep it consistent with the siblings' copies and note any intentional divergence.
5. **Keep rskit generic.** rskit is a foundational,
   multi-purpose framework that any project can consume —
   never make a capability consumer-specific (gokit-, pykit-, or Toven-specific) to satisfy one downstream.
   If a downstream exposes a gap, improve rskit generically.

## Naming and cross-references

- Crate/module names align across kits (rskit `logging` ↔ gokit `logging` ↔ pykit `logging`).
  Preserve the shared naming; call out any deliberate rename in the parity matrix.
- In PR/issue text, reference items in a sibling repo (or any other repo) with **full URLs**,
  never a bare `#123` — a bare number resolves to the current repo.
- Do not name branches/commits/PRs after internal plan or batch numbers;
  name by the actual capability change. Each PR must read standalone.

## Validate

Run the affected rskit crates through the `validate` skill and,
for a real audit of the parity claim, the `review` skill:

```bash
make test C=<crate>
```

Per repo workflow, **create the branch and make edits only** — the maintainer commits and pushes.
