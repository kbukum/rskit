---
name: create-pr
description: "rskit: Open a draft pull request using the repository template; only when explicitly asked."
---

# Opening a reviewer-friendly pull request

A PR is a reviewer's entry point, not a changelog of your keystrokes. This skill turns a pushed branch into a PR whose description explains the change **at a high level, organized and simplified**, and whose template sections are filled **honestly** against rskit's baseline.

Create a PR **only when explicitly asked** — never as a side effect of finishing work.

## 1. Preconditions

- The branch is committed and **pushed** to `origin`. Commit and push only when authorized (see the `commit` skill). Confirm before opening:

```bash
git rev-parse --abbrev-ref HEAD
git status --short                     # expect clean; uncommitted work is not in the PR
git log --oneline origin/main..HEAD    # the commits this PR will contain
```

- Base is `main`. If the branch isn't on the remote yet, ask before pushing.

## 2. Understand the change at a high level

Read the actual diff and group it by concern — do not narrate per file:

```bash
git diff origin/main...HEAD --stat
git diff origin/main...HEAD            # skim for the shape of the change, not to transcribe it
```

Answer, in your head: what capability/fix/refactor is this, which crates it touches, what a reviewer must understand to judge it, and whether it changes a public abstraction (error codes, `Component` lifecycle, `Provider`/stream shapes, typestate `App<S, C>`) that gokit shares as a contract.

## 3. Write the description — high level, organized, simplified

Fill every section of [`../../PULL_REQUEST_TEMPLATE.md`](../../PULL_REQUEST_TEMPLATE.md). The guiding rule: **a reviewer should grasp the change from the description alone**, without reconstructing it from the diff.

- **Title** — Conventional Commit style, naming the change: `feat(storage): add S3 multipart`, `refactor(di): typed cycle detection`. No plan/batch/step numbers or internal sequencing — the PR stands alone.
- **Description** — a few sentences of *what changed and why it's shaped this way*, at the level of capabilities and decisions, not code lines.
- **Motivation** — the problem it solves. Link issues as `Fixes #123`; reference **other repos as full URLs** (e.g. `https://github.com/kbukum/gokit/issues/45`), never a bare `#45`.
- **Type of Change / Crate(s) Affected** — mark accurately.
- **Changes Made** — a short, grouped bullet list of the *key* changes by concern (e.g. "typed registry factory", "in-memory default kept in core"), not one bullet per file and not a commit log.
- **Testing** — check only the gates you actually ran; scope to affected crates. These map to `make`/`cargo` (see the `validate` skill): `make test C=<crate>`, `make lint C=<crate>`, `make test-affected`. Paste real evidence if useful; don't fabricate output.
- **Breaking Changes** — pre-stable, describe the redesign, not a migration shim.
- **Sibling Parity** — no kit is the canonical reference; if a public abstraction changed, note whether gokit needs the same contract change and link the sibling item as a full URL; otherwise mark parity-not-required.
- **Checklist** — tick only what is genuinely true. An unchecked box is honest signal; a falsely checked one wastes reviewer trust. Do not narrate prior bugs or how they were fixed — describe the change as it stands.

Keep prose tight: no process narration, no "previously we…", no restating the diff. Explain the *current* state and the decisions a reviewer needs.

## 4. Create it with `gh`

Write the body to a file so formatting survives, then open a **draft** PR against `main`:

```bash
gh pr create --draft --base main --title "<conventional-title>" --body-file <path>
```

- Do **not** add reviewers or request Copilot review unless explicitly asked.
- Report the PR URL back. Never post review replies under the maintainer's name.

## Baseline

The PR asserts the change meets the [engineering standard](../../engineering.md#definition-of-done); if it doesn't yet, run the `review` skill first and fix findings rather than opening a PR that fails its own checklist.
