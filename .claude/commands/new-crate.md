---
description: 'Scaffold a new crate in the rskit multi-workspace monorepo the canonical way — decide core vs contrib, wire the workspace Cargo.toml, inherit workspace metadata, add #![warn(missing_docs)], and wire the facade. Use when adding a new capability, foundation crate, or adapter to rskit, or when unsure whether new code belongs in core or contrib.'
---

# /new-crate — router to the canonical skill

This command is a **thin router**. The single source of truth for this workflow is the
project skill at [`.github/skills/new-crate/SKILL.md`](../../.github/skills/new-crate/SKILL.md).

**Do this now:** read `.github/skills/new-crate/SKILL.md` in full — plus every reference file it
links — and execute it exactly as written, applying it to the scope below. Do not act on any
summary; the skill file is authoritative and kept up to date. This router only exists so the
Claude Code slash command and the Copilot skill never drift.

Scope / arguments: $ARGUMENTS
