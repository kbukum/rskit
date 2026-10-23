---
name: docs
description: "rskit: Update or audit documentation for accuracy, clear prose, working examples, and links."
user-invocable: true
---

# Documentation

Apply the [documentation rules](../../engineering.md#documentation-and-consumer-experience). Scope to requested docs and directly affected references, including rustdoc and agent docs.

1. Verify commands against Makefile, crates/workspaces/facade against manifests, and owners against `docs/CONCERN-OWNERS.md`. Check API examples against source; consult sibling-parity only for parity claims.
2. Lead with the reader's task or capability, then the shortest working example. Use concise active language and clear headings. Keep detail for correct use, contracts, safety, or verification; remove repetition and implementation narration. Changelogs summarize outcomes. Use tables or diagrams only when clearer than prose.
3. Keep Markdown, rustdoc, and comment prose free of arbitrary column wrapping. Preserve paragraphs, lists, directives, code blocks, and meaningful hard breaks; no blind line joining.
4. Remove stale current-usage guidance, not historical changelog entries or accepted ADRs. Stable docs must not link to temporary plans.
5. Check links/anchors and compile executable examples. `make doc C=<crate>` checks rustdoc; run scoped doctests separately when examples change. A docs build alone does not execute doctests. Prose-only edits need no application build.

For agent docs, use short rules and trigger-specific descriptions. Skills state actions and acceptance, linking canonical detail rather than repeating it. Preserve hard requirements and validate metadata/links.

Commit only when explicitly requested, using the commit skill.
