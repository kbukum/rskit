# Versioning Roadmap

This document records rskit's release-and-versioning model and the decision points that guide its evolution:

- [`VERSIONING.md`](VERSIONING.md) is the **current policy**.
- [`RELEASING.md`](RELEASING.md) is the **current mechanical runbook**.
- This roadmap keeps future release-model changes deliberate rather than ad hoc.

## Background: the two industry models

Large multi-crate Rust workspaces converge on one of two versioning models. Both are legitimate;
the choice is a trade-off, not a correctness question.

| Model | What it means | Representative projects |
|---|---|---|
| **Lock-step (unified)** | Every publishable crate shares one version and is released together. | Tokio family, Bevy |
| **Independent (per-crate)** | Each crate bumps only when it changes; dependents cascade. | serde / serde_derive, clap, prost, tracing |

**Lock-step** optimizes for a simple compatibility story and minimal release tooling, at the cost of republishing unchanged crates on every release.

**Independent** optimizes for meaningful per-crate history and minimal churn, at the cost of managing a compatibility matrix and internal dependency pins — which is only sustainable with automation.

A facade-style suite (rskit ships `rskit-suite`, imported as `rskit`) can lean toward lock-step, because consumers pin the facade and expect the family to move together. rskit uses **independent per-crate** versioning in `0.x` because its crate count (70+ publishable crates) makes full-suite republishing the dominant release cost; caret pins keep the facade's compatibility story simple by absorbing dependency patches.

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

The rskit release flow is driven by [Toven](https://github.com/kbukum/toven) through the `make release-*` targets. It keeps the release path explicit while the crate graph and versioning policy settle.

## Current model and future options

### Current — Independent per-crate versioning

- Each crate carries its **own** `version` (no `version.workspace = true`); all other `[workspace.package]` metadata is still inherited.
- Internal deps keep their **caret** `{ path, version }` pins. A patch bump is absorbed by the caret (no cascade); a 0.x **minor** bump leaves the caret range and cascades to in-workspace dependents.
- `core/` and `contrib/` are **independent release trains**; tooling operates **per workspace**. Cross-workspace propagation is deliberately minimal rather than absent: a breaking bump rewrites the affected dependency floor in **every** workspace manifest (e.g. a `core` breaking bump updates `contrib/Cargo.toml`), and the follow-up release for the dependent workspace selects the crates that inherit the changed floor and republishes them — there is no single combined pass that bumps both workspaces at once.
- Bumps are computed by Toven from the Conventional-Commit history (`toven release plan`, driven through `make release-plan`/`make release-bump`): patch by default, minor for a breaking change, idempotent against the crates.io max published version. Toven's publisher then republishes only the new `name@version`s.

**Why:** republishing only changed crates (plus the correct minimal cascade) keeps releases fast and within rate limits, while caret pins keep version drift low — drift lives in patch, not in major/minor. This is the serde / clap / tracing model, chosen because the crate count makes full-suite republish the dominant cost even in 0.x.

#### Where major breaks come from (why simple tooling suffices)

The common case is **patch-local** (absorbed by carets), so the tooling needs no heavy cascade engine:

1. **Foundation vocabulary** (`util`, `errors`, `component`, `di`, ...) grows additively → near-zero organic majors.
2. **Contract / trait crates** (`provider`, `pipeline`, `resilience`, ...) carry the breaking risk, but it is front-loaded into the pre-1.0 redesign, then frozen.
3. **Adapters** (`contrib/*`) are thin wrappers that expose only `Config` + `register(&mut Registry, Config)` and keep the SDK-backed struct private, so upstream SDK majors usually stay internal (a patch for us).

**Watch-list** — the few crates that may surface a foreign type in their public API, where an upstream major can leak out as our major; guard with API-stability discipline:

- `rskit-grpc` (tonic `Status`/transport)
- `rskit-http` (Tower/axum)
- `rskit-server`, `rskit-sse` (axum)
- `rskit-git` (libgit2 / git2)
- `rskit-httpclient` (reqwest, if exposed)

### Future option — Split core and contrib into separate repos

- Core has **zero references to contrib** and the dependency direction is strictly `contrib -> core` (acyclic), so splitting them is architecturally simple.
- After the split, contrib consumes core like any external user (crates.io versioned deps), so the cross-workspace concern disappears entirely.
- `contrib -> core` deps carry a real caret version with `path` as local-only convenience, so the split is a no-op on published artifacts (delete the `path =` lines or swap to a submodule).
- Split-time details: local dev of contrib against unreleased core needs `[patch.crates-io]` or a git submodule; any adapter-surfacing facade belongs on the **contrib** side, not core.

### Optional decision — adopt `release-plz`

Toven already provides Conventional-Commit-driven bumps and an idempotent publisher over the current model. If automated changelogs and Release-PR flows become worthwhile beyond what Toven offers, `release-plz` supports per-crate mode and could complement or replace the bump computation without changing the publishing model.

## Decision points to revisit

- **Tag `1.0`:** the model works in `0.x`; `1.0` is a separate stability commitment that only changes whether additive growth lives in patch (`0.x`) or minor (`1.x`).
- **When to split repos:** when contrib's cadence clearly diverges from core or the repo size warrants independent CI/release.
- **When to adopt `release-plz`:** when manual changelog/commit classification becomes a recurring source of release friction.

## Summary

| | Current model | Future split option |
|---|---|---|
| Version model | Independent per-crate | Independent per-crate |
| Repos | one | core + contrib split |
| Tooling | Toven (`make release-*`) | per-repo release |
| Tags | one `v*` | per-repo |
| Publishes | only changed + cascade | only changed + cascade |

The throughline: **the idempotent, skip-by-`name@version` publisher supports the current and future models**, so this roadmap is a series of versioning-policy and repository-structure decisions, not a publishing rewrite.

## References

- [Versioning guide (current policy)](VERSIONING.md)
- [Releasing guide (current runbook)](RELEASING.md)
- [SemVer policy](policy/SEMVER.md)
- [release-plz](https://release-plz.ieni.dev/)
- [cargo-release](https://crates.io/crates/cargo-release)
- [Cargo SemVer compatibility](https://doc.rust-lang.org/cargo/reference/semver.html)
