---
description: Add a pluggable backend/adapter (storage, cache, messaging, inference, llm, media, vectorstore) to rskit the canonical way — a contrib crate under contrib/<domain>/<name> implementing the core trait, selected via config through an explicit typed registration, no import-time side effects, with the in-memory/local default kept in core. Use when integrating a provider like S3, Kafka, Redis, Qdrant, or an LLM/inference provider.
---

# /new-backend — router to the canonical skill

This command is a **thin router**. The single source of truth for this workflow is the
project skill at [`.github/skills/new-backend/SKILL.md`](../../.github/skills/new-backend/SKILL.md).

**Do this now:** read `.github/skills/new-backend/SKILL.md` in full — plus every reference file it
links — and execute it exactly as written, applying it to the scope below. Do not act on any
summary; the skill file is authoritative and kept up to date. This router only exists so the
Claude Code slash command and the Copilot skill never drift.

Scope / arguments: $ARGUMENTS
