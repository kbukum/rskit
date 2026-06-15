# Versioning Roadmap

This document describes how rskit's release-and-versioning model is expected to evolve, and the reasoning behind it. It is forward-looking on purpose:

- [`VERSIONING.md`](VERSIONING.md) is the **current policy**.
- [`RELEASING.md`](RELEASING.md) is the **current mechanical runbook**.
- This roadmap explains **where the model is going and why**, so future changes are deliberate rather than ad hoc.

## Background: the two industry models

Large multi-crate Rust workspaces converge on one of two versioning models. Both are legitimate; the choice is a trade-off, not a correctness question.

| Model | What it means | Representative projects |
|---|---|---|
| **Lock-step (unified)** | Every publishable crate shares one version and is released together. | Tokio family, Bevy |
| **Independent (per-crate)** | Each crate bumps only when it changes; dependents cascade. | serde / serde_derive, clap, prost, tracing |

**Lock-step** optimizes for a simple compatibility story and minimal release machinery, at the cost of republishing unchanged crates on every release.

**Independent** optimizes for meaningful per-crate history and minimal churn, at the cost of managing a compatibility matrix and internal dependency pins — which is only sustainable with automation.

A facade-style suite (rskit ships `rskit-suite`, imported as `rskit`) leans naturally toward lock-step early, because consumers pin the facade and expect the family to move together.

## How publishing decides what to release

This is the key invariant that makes the roadmap safe to execute: **the release tag does not decide what gets published.**

- The `v*` tag only selects the commit to check out and must be reachable from `main`.
- `make release-publish` enumerates **all** publishable crates (`core/` + `contrib/`, facade last) in dependency order.
- The publisher is **idempotent per `name@version`**: for each crate it checks crates.io and **skips** any `name@version` that already exists, otherwise it **publishes**.

So the publish set is driven entirely by **version-in-`Cargo.toml`** versus **version-on-crates.io** — never by a diff and never by the tag. The same publisher therefore supports both models with **no code changes**:

- Under lock-step, a version bump makes every crate a new `name@version`, so all publish.
- Under independent versioning, only bumped crates (and cascaded dependents) are new, so only those publish; everything else is skipped automatically.

## Tooling: where the ecosystem is in 2025

Modern multi-crate workspaces automate releases from **Conventional Commits** rather than hand-editing versions:

| Tool | Style | Fit |
|---|---|---|
| [`release-plz`](https://release-plz.ieni.dev/) | CI-driven, Release-PR flow | De-facto standard for serious multi-crate workspaces; the Rust analogue of JS changesets. Detects changed crates, bumps + cascades, updates internal pins, generates per-crate changelogs, publishes only affected crates. Supports both unified and per-crate modes via config. |
| [`cargo-release`](https://crates.io/crates/cargo-release) | CLI-first, local | Good for small teams / infrequent manual releases. Flexible but someone must drive it. |
| [`cargo-smart-release`](https://github.com/GitoxideLabs/gitoxide) | CLI, change-aware | "Release only what changed + dependents"; less widely adopted than release-plz. |

The current rskit release flow is a bespoke `scripts/rskit_tool` publisher plus Make targets. That is appropriate for the first releases, but is not where the ecosystem keeps long-lived multi-crate projects.

## The roadmap

### Stage 1 — Lock-step alpha/beta (current)

- All crates inherit one workspace version (`version.workspace = true`).
- One `v*` GitHub Release per repository release.
- Bumps are manual / Make-driven; the idempotent publisher handles resume.

**Why:** during `0.x` the API surface moves as a unit, the compatibility matrix is not yet worth managing, and lock-step keeps the first releases predictable. This matches Tokio and Bevy.

### Stage 2 — Adopt `release-plz` while still lock-step

- Run `release-plz` in **unified-version (workspace) mode**.
- Keep existing release gates, publish dry-run, SBOM generation, and signing.
- Gain Conventional-Commit-driven bumps, automated changelog, and a Release-PR flow — without hand-editing versions.

**Why:** removes the most error-prone manual step (version bumping + changelog) and aligns tooling with the ecosystem, while preserving the simple lock-step compatibility story. No change to the compatibility model, so it is low-risk.

### Stage 3 — Independent per-crate versioning (at/after 1.0)

- Flip `release-plz` to **per-crate mode** via config; crates carry their own versions and bump only when they change, with dependent cascade.
- Tags become per-crate (e.g. `rskit-errors-v0.3.1`) rather than a single repo `v*`.
- The existing publisher needs **no changes** — its skip-by-`name@version` logic already supports this.

**Why:** once crates stabilize at different cadences, republishing the whole suite for a single-crate patch becomes real friction and obscures per-crate history. This is the serde / clap / tracing model, appropriate once the API is settled enough to maintain a compatibility matrix.

## Decision points to revisit

- **When to start Stage 2:** as soon as manual version/changelog edits become a recurring source of release friction or mistakes.
- **When to start Stage 3:** when at least one crate clearly needs an independent cadence, or when full-suite republishing on small changes becomes the dominant cost.
- **Build/tag vs. crate versions:** keep the `v*` repo tag only while lock-step; retire it in favor of per-crate tags when Stage 3 lands.

## Summary

| | Stage 1 (now) | Stage 2 | Stage 3 |
|---|---|---|---|
| Version model | Lock-step | Lock-step | Independent |
| Tooling | Make + `rskit_tool` | `release-plz` (unified) | `release-plz` (per-crate) |
| Tags | one `v*` | one `v*` | per-crate |
| Publishes | all crates on bump | all crates on bump | only changed + cascade |
| Publisher changes | — | none | none |

The throughline: **the idempotent, skip-by-`name@version` publisher already supports every stage**, so this roadmap is a series of versioning-policy and tooling decisions, not a publishing rewrite.

## References

- [Versioning guide (current policy)](VERSIONING.md)
- [Releasing guide (current runbook)](RELEASING.md)
- [SemVer policy](policy/SEMVER.md)
- [release-plz](https://release-plz.ieni.dev/)
- [cargo-release](https://crates.io/crates/cargo-release)
- [Cargo SemVer compatibility](https://doc.rust-lang.org/cargo/reference/semver.html)
