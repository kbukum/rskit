---
name: docs
description: "rskit: Update or audit documentation for accuracy, clear prose, working examples, and links."
user-invocable: true
---

# Documentation

Apply the [baseline](../../copilot-instructions.md). Scope to requested docs and directly affected references, including rustdoc and agent docs.

1. Verify commands against Makefile, crates/workspaces/facade against manifests, and owners against `docs/CONCERN-OWNERS.md`. Check API examples against source; consult sibling-parity only for parity claims.
2. Lead how-to pages with a working example. Use plain active sentences, meaningful headings, and tables for options. Add a focused captioned Mermaid diagram only when useful.
3. Keep Markdown, rustdoc, and comment prose free of arbitrary column wrapping. Preserve paragraphs, lists, directives, code blocks, and meaningful hard breaks; no blind line joining.
4. Remove stale current-usage guidance, not historical changelog entries or accepted ADRs. Stable docs must not link to temporary plans.
5. Check links/anchors and compile executable examples. `make doc C=<crate>` checks rustdoc; run scoped doctests separately when examples change. A docs build alone does not execute doctests. Prose-only edits need no application build.

For agent docs, use short always-loaded rules and trigger-specific descriptions. Put task actions/acceptance in the skill and load supporting sections only when needed. Preserve hard requirements and validate metadata/links.

Commit only when explicitly requested, using the commit skill.
