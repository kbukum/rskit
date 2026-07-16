---
name: create-plan
description: >-
    Turn a non-trivial change into a written, reviewable plan under the repo's gitignored tmp/
    folder — a README overview plus, when the work is multi-step, numbered step markdown files
    that can be applied iteratively. Every plan is bound to rskit's engineering baseline. Use
    when scoping a feature, refactor, parity port, or release, or when asked to plan or break
    down work.
---

# Planning rskit work as applyable step files

A plan is a written contract for a change set: what to do, in what order, and how you will know
each part is done. In this repo a plan is **not prose to admire** — it is a folder of markdown
that the `apply-plan` / `apply-step` skills execute iteratively.

## Where plans live: `tmp/<plan-name>/`

Always create plans under `tmp/` at the repo root. `tmp/` is **gitignored** — plans are local
working scratch, never committed and never shipped. Name the folder by the change itself in
kebab-case (`tmp/typestate-app-lifecycle/`, `tmp/storage-s3-multipart/`) — the same high-level
naming rule as branches: no `batch-N`, plan numbers, or internal/session detail in the folder
name.

```bash
mkdir -p tmp/<plan-name>
```

## Structure

Match this shape (the same layout every plan folder under `tmp/` uses):

- **`README.md`** (always) — the overview: goal, how to read the folder, an ordered index of the
  step files with their dependency order, and the cross-cutting rules that apply to every step.
- **`NN-topic.md`** step files (when the work is multi-step) — zero-padded and ordered by
  dependency layer (`01-core.md`, `02-...`), each a self-contained unit of work. A genuinely
  small single-shot change can be one `README.md` with an inline step list instead of separate
  files — split into step files as soon as the work is iterative or spans crates.

Numbering orders the plan; it is **internal to the plan folder only**. When a step becomes a
branch/PR, name that branch/PR by the change (see the `create-branch` skill) — never `step-3` or
`batch-N`.

### Each step file contains

```markdown
# <Step title — the change, not "step N">

**Layer:** L<n> · **Depends on:** <steps> · **Blocks:** <steps> · **Status:** pending

## Scope
What this step changes and, explicitly, what it does not.

## Steps
1. Numbered, concrete actions at real file paths.
2. ...

## Files touched
- `core/rskit-x/**`, new `contrib/<domain>/y/**`, ...

## Acceptance criteria
- [ ] Behavior written test-first; green under race/shuffle/parallel on affected crates.
- [ ] <step-specific, verifiable outcomes>
```

`Status: pending` and the `- [ ]` boxes are the progress signal `apply-plan` reads to find the
first unfinished step. `apply-step` flips them to `done`/`- [x]` when a step lands.

## Bind every plan to the baseline

A plan may **not** invent a lighter standard than rskit's. Its cross-cutting rules restate — and
link to — the engineering baseline in [`../../copilot-instructions.md`](../../copilot-instructions.md)
and defer detailed judgment to the `review` skill's eight passes. In every plan's README, make
these load-bearing (not decorative):

- **Test-first (TDD).** Each behavior gets a failing test first, then minimal code, then refactor
  while green — failure paths included. Never batch production code and bolt tests on later.
- **Structure & placement.** Correct crate (`core/rskit-<name>` vs `contrib/<domain>/<name>`);
  acyclic layering (lower crates never depend on higher); every crate carries
  `#![warn(missing_docs)]` and is wired into the workspace + facade.
- **Canonical reuse.** Reuse or enhance the owning core crate / std before writing new code; never
  duplicate a shared concern.
- **Typed & minimal APIs.** No broad `Any`/`Box<dyn Any>` on public surfaces (documented opaque
  exceptions only); typed `AppError`/`AppResult` that preserve cause; timeouts + cancellation on
  remote calls.
- **Root-cause, no shims.** Pre-stable: redesign cleanly and remove the old path; no compat
  shims or half-migrations.
- **Readable files.** Split by concern into focused files — never pile into one file.
- **Composition.** Injected registries/config; no import-time side effects, no mutable global
  registries; inject logger/tracer/policies.

Order steps so each starts only when its dependencies are green, and so each maps to a
**standalone, reviewable change** — a reviewer seeing one step's diff should need no knowledge of
the plan's sequencing.

## Handoff

Creating the plan is a docs-only act under `tmp/` — no source edits, no branch, no commit. Apply
it later with the `apply-plan` skill (whole plan) or `apply-step` (one step).
