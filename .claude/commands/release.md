---
description: Cut a release of the rskit multi-workspace monorepo — decide the semver bump, update the CHANGELOG, let Toven derive independent per-crate version bumps, run the full pre-release gates and supply-chain sweep, and publish to crates.io in dependency order. Use when preparing or publishing an rskit release or checking release readiness.
---

# /release — router to the canonical skill

This command is a **thin router**. The single source of truth for this workflow is the
project skill at [`.github/skills/release/SKILL.md`](../../.github/skills/release/SKILL.md).

**Do this now:** read `.github/skills/release/SKILL.md` in full — plus every reference file it
links — and execute it exactly as written, applying it to the scope below. Do not act on any
summary; the skill file is authoritative and kept up to date. This router only exists so the
Claude Code slash command and the Copilot skill never drift.

Scope / arguments: $ARGUMENTS
