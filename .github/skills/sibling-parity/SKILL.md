---
name: sibling-parity
description: >-
    Keep rskit aligned with its sibling kit gokit (Go) — judge cross-kit parity
    by capability, mirror the strongest existing implementation for a given scope, track parity
    through gokit tracking issues, and keep each kit generic (never consumer-specific). Use when
    touching anything with a cross-kit parity row or aligning a capability across kits.
---

# Keeping rskit aligned with its sibling kit

rskit is a sibling kit to gokit (Go): the same capabilities and the same engineering baseline, each expressed idiomatically per language. The goal of parity is **intuition transfer** — a user fluent in one kit should adapt to the other quickly because the concepts, shapes, and behavior line up. This skill keeps that relationship honest as rskit changes. The sibling lives in `../gokit`.

**No kit is the canonical reference.** Parity is judged per capability, and for each scope the kit with the *better, more complete, more correct* implementation is the one the other mirrors. Parity levels the kits **up, never down** — never weaken a stronger implementation to match a weaker sibling. When rskit is stronger in a scope, propagate outward to gokit; when gokit is stronger, bring rskit up to it rather than assume rskit leads.

## Parity is scoped and capability-first, not symbol-for-symbol

Cross-kit parity weighs where each language is strongest — it is **not** a demand that every rskit symbol appear in gokit. Decide per capability:

- **Shared infrastructure** (errors, config, di, provider shapes, resilience, transport, data adapters) is expected to exist in both kits; keep the abstraction and naming aligned, matching whichever kit currently has the best version of that capability.
- **Language-strongest capabilities stay kit-only or "light" elsewhere.** Heavy media/video/audio/matrix work is Rust-strongest and belongs in rskit; gokit `media` is deliberately **light** (detection, metadata, cheap image ops, time/spatial types, subtitles). Record these as intentional divergences, not gaps.
- **Framework- or language-specific concepts** (e.g. rskit `http` is Axum-specific and folds into gokit `server`; gokit `connect` is ConnectRPC-specific with no rskit peer) are deliberate divergences with a note, not gaps to close.

## Idiom beats structural mimicry

Parity is about **capability and behavior**, not identical names or directory layouts. Where a language's current idioms, conventions, or best practices differ, follow that language's convention rather than force structural sameness — naming, crate/module organization, error/option ergonomics, and layout should read as native to each kit. Match the *concept and behavior* so users get intuition transfer; do not transliterate across languages. Call out any deliberate rename or reshaping in the parity matrix so the mapping stays discoverable.

## Workflow when a shared capability changes

1. **Find the strongest existing implementation.** Locate the counterpart crate/package/module in `../gokit` and its public API, invariants, and error model, then decide which side currently has the better implementation for this scope.
2. **Decide the mirroring level** (full / light / kit-only) and direction (propagate rskit outward, or bring rskit up to a stronger gokit) using the rules above.
3. **Propagate to whichever kit is behind.** When a shared abstraction is stronger in one kit, flag the change the weaker kit needs (an issue/note in the sibling repo, referenced by **full URL**), or align it if that is in scope. Do not silently let the kits drift, and do not assume the change always flows out of rskit.
4. **Track parity in gokit's tracking issues.** When a public abstraction changes, open or annotate a tracking issue in the gokit repo (https://github.com/kbukum/gokit) recording the capability, the mirroring level, and any intentional divergence. Reference it by full URL.
5. **Keep every kit generic.** Each kit is a foundational, multi-purpose framework that any project can consume — never make a capability consumer-specific (gokit- or Toven-specific) to satisfy one downstream. If a downstream exposes a gap, improve the owning kit generically.

## Naming and cross-references

- Crate/module names align across kits **where the idiom allows** (rskit `logging` ↔ gokit `logging`). Preserve shared naming when it is natural per language; call out any deliberate, idiom-driven rename in the gokit tracking issue.
- In PR/issue text, reference items in a sibling repo (or any other repo) with **full URLs**, never a bare `#123` — a bare number resolves to the current repo.
- Do not name branches/commits/PRs after internal plan or batch numbers; name by the actual capability change. Each PR must read standalone.

## Validate

Run the affected rskit crates through the `validate` skill and, for a real audit of the parity claim, the `review` skill:

```bash
make test C=<crate>
```

Per repo workflow, **create the branch and make edits only** — the maintainer commits and pushes.
