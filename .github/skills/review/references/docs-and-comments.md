# Lens: docs and comments

rustdoc and READMEs are the contract users read. A stale or narrative comment is worse than none.

Rules: [documentation and consumer experience](../../../engineering.md#documentation-and-consumer-experience), [docs skill](../../docs/SKILL.md).

## What to look for

- **rustdoc.** Every public item documented with its contract, `# Errors`, and `# Panics` where reachable. Crate `//!` docs state purpose and place in the toolkit. Examples compile.
- **Comments.** Explain why, not what. Describe the code as it is: no plan numbers, PR references, "now", "previously", "for now", or restated code. Actionable `TODO`s name what and why.
- **`SAFETY:` comments.** State the real invariant.
- **Docs.** Task-first, working examples, correct commands and links. No hard-wrapped prose. Stable docs never link to `tmp/`.
- **Claims.** A doc never says a rule is enforced unless a gate checks it.
- **Changelog.** Outcomes under `[Unreleased]`; breaking changes marked.

## Severity

| Severity | Examples |
|---|---|
| Blocker | Comment or doc that contradicts the code; false enforcement claim |
| Should-fix | Missing rustdoc on public items; process narration; broken link or stale command; hard-wrapped prose |
| Nit | Wording that could be shorter |

## Detection starters

```bash
rg -n 'Decision [0-9]|step [0-9]|phase [0-9]|review pass|PR #[0-9]' core/*/src contrib/*/*/src
rg -n -w 'newly|recently|previously|used to|for now|temporary' core/*/src contrib/*/*/src
rg -n 'tmp/' --glob '*.md' --glob '!tmp/**' --glob '!target/**' .
make doc C=<crate>
make test-doc C=<crate>
```
