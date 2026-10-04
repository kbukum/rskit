# rskit

Rust infrastructure kit. `core/rskit-*` holds foundations and the facade; `contrib/<domain>/<name>` holds adapters; `examples/` holds consumers. These are separate Cargo workspaces. Read toolchain and dependency versions from their manifests.

## Invariants

- Pre-stable: fix root causes with Redesign / Align / Enhance / Drop, not compatibility shims. Preserve sound code and keep the change's dependent callers consistent.
- Consult [concern owners](../docs/CONCERN-OWNERS.md) before adding shared logic. Imports point downward; enhance a lower owner before consuming it. Kits are runtime-independent; consistency and idiomatic Rust outrank symbol parity.
- Typed, minimal public APIs; preserve causes through `AppError` / `AppResult`. No runtime `unwrap`, `expect`, swallowed errors, or success-shaped fallbacks.
- Config selects explicit injected registries/adapters. No import-time I/O or global mutable registries; inject telemetry, clients, and policies. Provider shapes: RequestResponse, Stream, Sink, Duplex.
- Validate trust boundaries; no secrets in code/logs, credential URLs, SQL interpolation, or shell-built subprocesses. Bound remote calls, idempotent jittered retries, buffers, and tasks; own cancellation and shutdown.
- `lib.rs` / `mod.rs` contain declarations and re-exports only. Use concern-named modules; split by responsibility, not a hard line count. Inherit workspace lints, document public items, mark `with_*` builders `#[must_use]`, and use `#[non_exhaustive]` for growing public enums. Follow the repository's unsafe policy.
- Test-first and deterministic; reuse doubles, cover failures, and use paused Tokio time, not sleep. Serialize environment mutation. Coverage: >=80% per package, >=85% overall and for errors/auth/authz/security/resilience/encryption. Keep integration proof.
- Markdown, rustdoc, and comment prose have no arbitrary column wrapping; preserve code examples and directives.

## Work and validation

Load only the matching [skill](skills/README.md) and reference sections. Preserve worktree/index changes. Commit, amend, push, or open draft PRs only when authorized. Multi-step work uses `tmp/plans/<task>/handoff.md`; resume current scope and dependency contracts, not every old step.

Use `make test C=<crate> T=<pattern>`, `make lint C=<crate>`, and `make doc C=<crate>` as relevant. `make check` is the full gate; see [validate](skills/validate/SKILL.md) for topology, API, coverage, and dependency gates. Prose-only edits need documentation checks, not a full build.

## Load before the relevant change

| Change | Reference |
|---|---|
| New crate, facade, or adapter | [Crate structure](engineering.md#crate-structure), then `new-crate` / `new-backend` |
| APIs, locking, module structure, errors | [Code style](engineering.md#code-style) and [Key patterns](engineering.md#key-patterns) |
| Security, AI, dependencies, release | [Engineering principles](engineering.md#engineering-principles), then the matching checklist |

Read the needed section, not the entire reference. A test double proves a contract, not real integration behavior.
