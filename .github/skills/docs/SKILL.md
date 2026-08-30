---
name: docs
description: >-
    Review and update rskit's documentation so it reads naturally and reflects the toolkit as it is
    today — keep Markdown paragraphs flowing without hard column wrapping, preserve intentional
    document structure, sync commands, crate/workspace structure, and examples with the actual code,
    fix stale links and dead references, drop history/plan narration, keep prose humanized and
    scannable with a task-first quickstart, and add mermaid diagrams where they clarify architecture
    or flow. Use when writing or auditing
    docs, repairing AI-generated hard wraps, after a change that makes documentation outdated, or before a release.
user-invocable: true
---

# Reviewing and updating rskit's docs

Documentation can fail two ways: it can fall out of **standard** (arbitrary source line breaks, history narration, dead links), and it can become **out of date** (commands, crate lists, facade wiring, and examples that no longer match the code). This skill checks both. rskit is shared foundation infrastructure that its sibling kits and downstream consumers build on, so a stale doc misleads every downstream consumer — the standard is high. Run it over the whole `docs/` tree, a single file, or the docs touched by a change set.

The authoritative doc policy lives in the Documentation section of [`.github/copilot-instructions.md`](../../copilot-instructions.md) (and `docs/DESIGN.md`). The baseline wins over any local habit.

## Docs in scope

Check every committed prose source, not just `docs/`:

- `docs/**` — `DESIGN.md`, `PACKAGES.md`, `MODULE-INDEX.md`, `CONCERN-OWNERS.md`, `CONSUMER-CLASSES.md`, `EXAMPLES.md`, `VERSIONING*.md`, `RELEASING.md`, `security-model.md`, `PARITY-MATRIX.md`, the ADRs under `docs/adr/`, and dependency graphs under `docs/depgraphs/`.
- `README.md`, `CHANGELOG.md`, `MAINTAINERS.md`, and any top-level `*.md`.
- `.github/skills/**/SKILL.md` and their `references/*.md`.
- `///` rustdoc and `//` comments in the crates in scope (these are docs too).

Never touch `tmp/` (gitignored scratch) and never add a committed doc that references it.

## Pass 1 — Standards (how it reads)

- **Flowing Markdown prose.** A Markdown paragraph is one continuous source line. Do not hard-wrap prose to a column limit or add source newlines to control how it looks at one editor width; GitHub and other renderers wrap it for the reader's viewport. Collapse AI-generated hard wraps only within the same logical paragraph.
- **Preserve intentional structure.** Keep blank-line paragraph boundaries, headings, list items, blockquotes, tables, link definitions, HTML blocks, mermaid diagrams, and fenced or indented code blocks. Never join separate list items or paragraphs. Preserve hard line breaks that are semantically meaningful (`<br>` or two trailing spaces).
- **Rust documentation.** Write `//!`/`///` rustdoc and `//` prose naturally without arbitrary column-based breaks. Preserve directives, headings, lists, tables, and code examples. Do not join separate comment paragraphs. The `rustfmt` `max_width` limit is for code, not prose.
- **No history/plan/process narration.** A doc or comment describes the system as it is now, not how it got here or what a future plan intends. Delete "previously…", "we changed…", batch/plan/PR references, and TODO-narration.
- **`tmp/` stays uncommitted.** No committed doc references a `tmp/` plan or handoff note.
- **Frontmatter exemption.** YAML folded scalars (e.g. a skill's `description: >-`) already collapse to one logical line — leave their wrapping alone.

## Pass 2 — Up-to-date check (whether it's still true)

Verify each doc against the code it describes; a doc that lies is worse than no doc:

- **Commands & gates** match the `Makefile` (`make check`, `make lint C=<crate>`, `make test`, `make doc`, `make deny`, `make release-*`, `make check-topology`/`check-public-api`) — no renamed or removed target lingers in the docs.
- **Crate & workspace structure** matches reality: the `core/`/`contrib/`/`examples/` split, the crate lists in `PACKAGES.md`/`MODULE-INDEX.md`, and facade wiring in the `rskit` umbrella crate match the tree; renamed/added/dropped crates are reflected everywhere they appear (including `CONCERN-OWNERS.md`).
- **Canonical-owner claims** are accurate: `CONCERN-OWNERS.md` names the crate that actually owns each concern today.
- **Parity matrix** is current: `PARITY-MATRIX.md` reflects what gokit/pykit currently mirror (see the `sibling-parity` skill).
- **Examples run.** Code/command examples reflect current behavior; doctests compile under `make doc`.
- **Links resolve.** Internal relative links and cross-references point at files that exist; other-repo references use full URLs, never bare `#123`.

## Pass 3 — Clarity & developer experience (does it actually help the reader?)

Standards and accuracy make a doc correct; this pass makes it *usable*. Judge every doc by whether a developer under time pressure finds what they need and gets running fast — a correct doc nobody can skim has still failed.

- **Humanized, plain language.** Write for a developer skimming, not a spec lawyer. One idea per sentence; keep sentences short. Use active voice and direct instructions ("Call `App::new`", "Send a GET request"), never passive throat-clearing ("a request should be sent"). Cut filler and hedging. Prefer the plain word over jargon; define an unavoidable term the first time it appears.
- **Scannable, uncrowded structure.** Let the reader find the answer by scanning, not by reading top to bottom. Break content with meaningful headings, short lists, and tables; bold the load-bearing terms; keep paragraphs to a few sentences. Whitespace and sectioning carry meaning — never a wall of text.
- **Task-first, quickstart up top.** Order each doc by what the reader wants to do, most common first (inverted pyramid). Lead with the shortest copy-pasteable path to a first working result, before deep reference. Know which of the four Diátaxis modes each page is — tutorial, how-to, reference, or explanation — and don't blend them on one page.
- **Real, runnable examples.** Every non-trivial capability shows a real, copy-pasteable snippet that compiles against the current API — never pseudo-code. Show the common path first, then the important options and failure cases. Prefer runnable `///` doctests so the compiler keeps them honest.
- **Diagrams where prose is the wrong tool.** When a doc explains architecture, layer/dependency direction, a request or data flow, a state machine, or component interaction, add a focused `mermaid` diagram right where the concept is introduced — a diagram earns its place only by replacing a paragraph the reader would otherwise assemble in their head. Keep each diagram to one idea (prefer several small diagrams over one crowded one), pair it with a one-line caption so it degrades where mermaid isn't rendered, and keep it in sync with the code like any other doc. Don't diagram the trivial.
- **Every element earns its place.** Delete restated-obvious prose, duplicate explanations, and decoration that doesn't help someone build. Meaning over volume.

## Apply, then validate

Fix every instance of a pattern across the whole scope, not just the first hit. When repairing hard wraps, read and judge the Markdown structure rather than applying a blind line-joining script. Then validate what you touched:

```bash
make doc C=<crate>                                  # rustdoc builds with -D warnings for the crate whose /// docs changed
```

Docs/prose-only changes need no build/test gate beyond `make doc` when rustdoc changed. Verify internal links by path before finishing.

## Commit

Use the [`commit`](../commit/SKILL.md) skill — one compact `docs:` Conventional-Commit line stating the change (e.g. `docs: repair hard-wrapped prose and sync PACKAGES.md`). No `Co-authored-by` trailer, no plan/batch/tool narration. Group by intent when it aids the reader (a prose-flow repair and an up-to-date content update read as separate commits).
