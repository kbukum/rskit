---
name: create-pr
description: >-
    Open a pull request that reads well for a reviewer — understand the change set at a high
    level, fill the repo PR template honestly, and keep the description a concise, organized,
    developer-friendly summary (no file-by-file dumps, no internal/batch/plan detail). Bound to
    rskit's engineering baseline. Use only when explicitly asked to create or open a PR.
user-invocable: true
---

# Opening a reviewer-friendly pull request

A PR is a reviewer's entry point, not a changelog of your keystrokes. This skill turns a pushed
branch into a PR whose description explains the change **at a high level, organized and
simplified**, and whose template sections are filled **honestly** against rskit's baseline.

Create a PR **only when explicitly asked** — never as a side effect of finishing work.

## 1. Preconditions

- The branch is committed and **pushed** to `origin` (the maintainer commits and pushes, per repo
  workflow). Confirm before opening:

```bash
git rev-parse --abbrev-ref HEAD
git status --short                     # expect clean; uncommitted work is not in the PR
git log --oneline origin/main..HEAD    # the commits this PR will contain
```

- Base is `main`. If the branch isn't on the remote yet, ask the maintainer to push rather than
  pushing for them.

## 2. Understand the change at a high level

Read the actual diff and group it by concern — do not narrate per file:

```bash
git diff origin/main...HEAD --stat
git diff origin/main...HEAD            # skim for the shape of the change, not to transcribe it
```

Answer, in your head: what capability/fix/refactor is this, which crates it touches, what a
reviewer must understand to judge it, and whether it changes a public abstraction (error codes,
`Component` lifecycle, `Provider`/stream shapes, typestate `App<S, C>`) that the sibling kits
(gokit/pykit) mirror.

## 3. Write the description — high level, organized, simplified

Fill every section of [`../../PULL_REQUEST_TEMPLATE.md`](../../PULL_REQUEST_TEMPLATE.md). The guiding
rule: **a reviewer should grasp the change from the description alone**, without reconstructing it
from the diff.

- **Title** — Conventional Commit style, naming the change: `feat(storage): add S3 multipart`,
  `refactor(di): typed cycle detection`. No plan/batch/step numbers or internal sequencing —
  the PR stands alone.
- **Description** — a few sentences of *what changed and why it's shaped this way*, at the level
  of capabilities and decisions, not code lines.
- **Motivation** — the problem it solves. Link issues as `Fixes #123`; reference **other repos as
  full URLs** (e.g. `https://github.com/kbukum/gokit/issues/45`), never a bare `#45`.
- **Type of Change / Crate(s) Affected** — mark accurately.
- **Changes Made** — a short, grouped bullet list of the *key* changes by concern (e.g. "typed
  registry factory", "in-memory default kept in core"), not one bullet per file and not a commit
  log.
- **Testing** — check only the gates you actually ran; scope to affected crates. These map to
  `make`/`cargo` (see the `validate` skill): `make test C=<crate>`, `make lint C=<crate>`,
  `make test-affected`. Paste real evidence if useful; don't fabricate output.
- **Breaking Changes** — pre-stable, describe the redesign, not a migration shim.
- **Sibling Parity** — rskit is the **reference** kit; if a public abstraction changed, note
  whether gokit/pykit need to mirror it and link the sibling item as a full URL; otherwise mark
  parity-not-required.
- **Checklist** — tick only what is genuinely true. An unchecked box is honest signal; a falsely
  checked one wastes reviewer trust. Do not narrate prior bugs or how they were fixed — describe
  the change as it stands.

Keep prose tight: no process narration, no "previously we…", no restating the diff. Explain the
*current* state and the decisions a reviewer needs.

## 4. Create it with `gh`

Write the body to a file so formatting survives, then open against `main`:

```bash
gh pr create --base main --title "<conventional-title>" --body-file <path>
```

- Do **not** add reviewers or request Copilot review unless explicitly asked.
- Report the PR URL back. If asked to follow up on review threads, resolve them without posting
  replies under the maintainer's name.

## Baseline

The PR asserts the change meets rskit's engineering baseline
([`../../copilot-instructions.md`](../../copilot-instructions.md)); if it doesn't yet, run the
`review` skill first and fix findings rather than opening a PR that fails its own checklist.
