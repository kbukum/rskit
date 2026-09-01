# rskit development skills

[Agent Skills](https://docs.github.com/copilot/concepts/agents/about-agent-skills) for developing **rskit itself** — loaded on demand by GitHub Copilot (CLI, coding agent, code review, IDEs) when a task matches a skill's description. These are **project skills** for contributors; they do not affect anyone who consumes rskit as a dependency.

Each skill is a folder with a `SKILL.md` (YAML frontmatter + workflow) and optional bundled reference files loaded only when the skill activates (progressive disclosure). They encode rskit's permanent engineering baseline (see [`../copilot-instructions.md`](../copilot-instructions.md) and `docs/DESIGN.md`) and drive tasks through the repo's `make`/`cargo` gates.

## Skills

| Skill | Use when |
|---|---|
| [`create-branch`](create-branch/SKILL.md) | Cut a branch off an up-to-date main, named by the high-level change (no batch/plan/internal detail). |
| [`create-plan`](create-plan/SKILL.md) | Turn a non-trivial change into a reviewable plan under `tmp/` — README + numbered step files, bound to the baseline. |
| [`apply-plan`](apply-plan/SKILL.md) | Execute a `tmp/` plan from its first unfinished step onward, validating after each; resumable. |
| [`apply-step`](apply-step/SKILL.md) | Apply one plan step in context (README + prior steps), test-first against the baseline, then mark it done. |
| [`commit`](commit/SKILL.md) | Commit staged work with one compact, developer-friendly Conventional-Commit message — no co-author trailer or plan/batch/tool narration. |
| [`create-pr`](create-pr/SKILL.md) | Open a reviewer-friendly PR — high-level summary, honest template sections, bound to the baseline. |
| [`fix-reviews`](fix-reviews/SKILL.md) | Act on PR review comments by pattern — fix every instance across the change set, then commit and resolve the threads. |
| [`validate`](validate/SKILL.md) | Build/test/lint/format-check/doc/deny a change through make/cargo, scoped to the affected crates. |
| [`review`](review/SKILL.md) | Run the eight-pass engineering-baseline review over a diff, crate, or the tree. |
| [`new-crate`](new-crate/SKILL.md) | Scaffold a new crate — core vs contrib placement, workspace Cargo.toml, `#![warn(missing_docs)]`, facade wiring. |
| [`new-backend`](new-backend/SKILL.md) | Add a storage/cache/messaging/inference/llm/media/vectorstore adapter as a typed-registration contrib crate. |
| [`release`](release/SKILL.md) | Cut a release — semver bump, CHANGELOG, per-crate bumps, full gates, crates.io publish in dependency order. |
| [`sibling-parity`](sibling-parity/SKILL.md) | Keep rskit aligned with gokit — mirror whichever kit is strongest per capability, propagate both ways, track parity through gokit tracking issues. |
| [`docs`](docs/SKILL.md) | Review/update docs to the repo's standards (flowing paragraphs without hard column wrapping) and keep them up to date (commands, crate structure, examples match the code). |

## Conventions

- Skills are discoverable in Copilot CLI via `/skills`; project skills live under `.github/skills/` (also `.claude/skills` / `.agents/skills` are honored), personal skills under `~/.copilot/skills`.
- Claude Code slash commands under [`.claude/commands/`](../../.claude/commands/) are **thin routers** to these skills — each `/<name>` points at `.github/skills/<name>/SKILL.md`, which is the single source of truth. Edit the `SKILL.md`, never the router body.
- Run reviews (`review`) in a **fresh, clean-context agent**, never inline in the session that wrote the code.
- Validation is `make`/`cargo`-first, scoped to the changed crate(s) (`make lint C=<crate>`, `make test C=<crate> T=<pattern>`, `make test-affected`); full-tree gates are for audits and releases.
