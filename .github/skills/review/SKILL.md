---
name: review
description: "rskit: Review a diff, package, or tree for architecture, correctness, security, tests, and documentation."
---

# Review

Review code and evidence, not the author's rationale. A plan sets scope; it never excuses a rule. Rules live in the [engineering standard](../../engineering.md).

Review directly. Launch an independent reviewer only when the user asks, and reevaluate its findings against the source before acting. Reviews are read-only unless fixes are authorized. Never commit, amend, push, resolve threads, or post replies without authorization.

## Modes

| Mode | Use when |
|---|---|
| [Changes](references/mode-changes.md) | Reviewing a diff, branch, or working tree |
| [Project](references/mode-project.md) | Auditing a crate, domain, workspace, or tree in priority order |
| [Review and fix](references/mode-fix.md) | The user asked for fixes too |

## Lenses

Apply each lens whose trigger matches, in this priority order. Load only those files. Placement blockers stop acceptance. For prose-only changes, use docs and gates.

| # | Lens | Trigger |
|---|---|---|
| 1 | [Placement and ownership](references/placement-and-ownership.md) | New or moved code, imports, crates, shared helpers |
| 2 | [Security and runtime](references/security-and-runtime.md) | Input, secrets, I/O, tasks, locks, errors, `unsafe`, AI |
| 3 | [Public API and stability](references/public-api.md) | Any `pub` item, error type, or version change |
| 4 | [Tests and proof](references/tests.md) | Any behavior change or test |
| 5 | [Code quality and redesign](references/code-quality.md) | Any code change; CLI output |
| 6 | [Docs and comments](references/docs-and-comments.md) | rustdoc, comments, READMEs, docs, changelog |
| 7 | [Dependencies and supply chain](references/supply-chain.md) | Manifests, lockfiles, CI, release |
| 8 | [Gates and enforcement](references/gates.md) | Every review; gate or tooling changes |

## Severity

| Severity | Meaning |
|---|---|
| Blocker | Broken behavior or contract, or a hard-rule violation. Stops acceptance. |
| Should-fix | A real defect in maintainability, clarity, or consistency. Fix in this change unless deferred with a reason. |
| Nit | Optional preference. Never blocks. |

## Findings

Write each finding on one line:

```text
<severity> — <file>:<line> — <defect and impact> — <rule link> — <action: Redesign | Align | Enhance | Drop, and how>
```

Group repeated patterns into one finding with all locations. List skipped checks and uncertainty. Grep hits are leads, not proof; never invent findings. Green gates are necessary, not sufficient.
