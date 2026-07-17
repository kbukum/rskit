---
name: docs
description: >-
    Review and update rskit's documentation so it obeys the repo's doc standards and reflects the
    toolkit as it is today — reflow hard-wrapped prose to one line per paragraph, keep commands,
    crate/workspace structure, and examples in sync with the actual make targets and core/contrib
    layout, fix stale links and dead references, and drop history/plan narration. Use when writing
    or auditing docs, after a change that outdated them, or before a release.
user-invocable: true
---

# Reviewing and updating rskit's docs

Documentation can fail two ways: it can fall out of **standard** (hard-wrapped prose, history narration, dead links) and it can become **out of date** (commands, crate lists, facade wiring, and examples that no longer match the code). This skill checks both. rskit is shared foundation infrastructure and the reference kit gokit/pykit mirror, so a stale doc misleads every downstream consumer — the standard is high. Run it over the whole `docs/` tree, a single file, or the docs touched by a change set.

The authoritative doc policy lives in the Documentation section of [`.github/copilot-instructions.md`](../../copilot-instructions.md) (and `docs/DESIGN.md`). The baseline wins over any local habit.

## Docs in scope

Check every committed prose source, not just `docs/`:

- `docs/**` — `DESIGN.md`, `PACKAGES.md`, `MODULE-INDEX.md`, `CONCERN-OWNERS.md`, `CONSUMER-CLASSES.md`, `EXAMPLES.md`, `VERSIONING*.md`, `RELEASING.md`, `security-model.md`, `parity-matrix.md`, the ADRs under `docs/adr/`, and dependency graphs under `docs/depgraphs/`.
- `README.md`, `CHANGELOG.md`, `MAINTAINERS.md`, and any top-level `*.md`.
- `.github/skills/**/SKILL.md` and their `references/*.md`.
- `///` rustdoc and `//` comments in the crates in scope (these are docs too).

Never touch `tmp/` (gitignored scratch) and never add a committed doc that references it.

## Pass 1 — Standards (how it reads)

- **One line per paragraph.** Prose is never hard-wrapped. Reflow any paragraph that was broken mid-sentence to fit a column into a single physical line; let editors soft-wrap. This applies to Markdown, `///` rustdoc, and `//` comments alike. The `rustfmt` `max_width` limit is for *code*, not prose.
- **Preserve structure verbatim.** Do not reflow inside fenced code blocks, tables, mermaid diagrams, or list-item continuations — only collapse wrapped paragraph prose. Keep list markers, headings, and link syntax intact.
- **No history/plan/process narration.** A doc or comment describes the system as it is now, not how it got here or what a future plan intends. Delete "previously…", "we changed…", batch/plan/PR references, and TODO-narration.
- **`tmp/` stays uncommitted.** No committed doc references a `tmp/` plan or handoff note.
- **Frontmatter exemption.** YAML folded scalars (e.g. a skill's `description: >-`) already collapse to one logical line — leave their wrapping alone.

## Pass 2 — Up-to-date check (whether it's still true)

Verify each doc against the code it describes; a doc that lies is worse than no doc:

- **Commands & gates** match the `Makefile` (`make check`, `make lint C=<crate>`, `make test`, `make doc`, `make deny`, `make release-*`, `make check-topology`/`check-public-api`) — no renamed or removed target lingers in the docs.
- **Crate & workspace structure** matches reality: the `core/`/`contrib/`/`examples/` split, the crate lists in `PACKAGES.md`/`MODULE-INDEX.md`, and facade wiring in the `rskit` umbrella crate match the tree; renamed/added/dropped crates are reflected everywhere they appear (including `CONCERN-OWNERS.md`).
- **Canonical-owner claims** are accurate: `CONCERN-OWNERS.md` names the crate that actually owns each concern today.
- **Parity matrix** is current: `parity-matrix.md` reflects what gokit/pykit currently mirror (see the `sibling-parity` skill).
- **Examples run.** Code/command examples reflect current behavior; doctests compile under `make doc`.
- **Links resolve.** Internal relative links and cross-references point at files that exist; other-repo references use full URLs, never bare `#123`.

## Apply, then validate

Fix every instance of a pattern across the whole scope (a single reflow fix implies checking every hard-wrapped file), not just the first hit. Then validate what you touched:

```bash
git grep -nP '.{101,}' -- 'docs/**/*.md' '*.md'   # candidates: over-long lines to inspect (code blocks/tables are fine)
make doc C=<crate>                                  # rustdoc builds with -D warnings for the crate whose /// docs changed
```

Docs/prose-only changes need no build/test gate beyond `make doc` when rustdoc changed. Verify internal links by path before finishing.

## Commit

Use the [`commit`](../commit/SKILL.md) skill — one compact `docs:` Conventional-Commit line stating the change (e.g. `docs: reflow prose to one line per paragraph and sync PACKAGES.md`). No `Co-authored-by` trailer, no plan/batch/tool narration. Group by intent when it aids the reader (a standards reflow sweep and an up-to-date content update read as separate commits).
