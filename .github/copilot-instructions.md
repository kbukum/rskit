# rskit

Pre-1.0 Rust infrastructure kit. `core/rskit-*` holds foundations and the `rskit-suite` facade; `contrib/<domain>/<name>` holds adapters; `examples/` holds consumers. Each is a separate Cargo workspace with no root `Cargo.toml`. Read toolchain and dependency versions from the manifests.

## Invariants

The [engineering standard](engineering.md) owns these rules; this is the short form.

- Pre-stable: fix root causes with Redesign, Align, Enhance, or Drop. No compatibility shims. Update dependent callers in the same change.
- Find the [concern owner](../docs/CONCERN-OWNERS.md) before adding shared logic. Imports point down. Idiomatic Rust and project consistency outrank gokit symbol parity.
- Typed, minimal public APIs. Errors are `AppError` / `AppResult` and keep their cause. No runtime `unwrap`, `expect`, `panic!`, swallowed errors, or success-shaped fallbacks.
- Config selects injected registries and adapters. No import-time I/O or global mutable registries.
- Validate trust boundaries. No secrets in code, logs, or URLs; no SQL interpolation or shell-built subprocesses. Bound remote calls, retries, buffers, and tasks; own cancellation and shutdown.
- `lib.rs` / `mod.rs` hold declarations and re-exports only.
- Test first, deterministic, with paused Tokio time. Reuse `rskit-testutil` doubles. Coverage floors live in `toven.toml`.
- Never hard-wrap Markdown, rustdoc, or comment prose.
- Docs and agent instructions are short and task-first. Skills link rules instead of repeating them.

## Work and validation

- Load only the matching [skill](skills/README.md) and the reference sections it needs.
- Preserve the worktree and index. Commit, amend, push, or open draft PRs only when authorized.
- Keep plans in `tmp/<plan>/`, reusing existing folders. Apply a selected step fully and record progress in it.
- Validate with the smallest covering gate (`make test C=<crate> T=<pattern>`, `make lint C=<crate>`, `make doc C=<crate>`), then `make check`. The [validate skill](skills/validate/SKILL.md) maps changes to gates. Prose-only edits need link checks, not a build.

## Where to read

| Change | Read |
|---|---|
| New crate, adapter, or shared helper | [Placement and ownership](engineering.md#placement-and-ownership), then `new-crate` or `new-backend` |
| Input, secrets, I/O, tasks, `unsafe` | [Security and runtime](engineering.md#security-and-runtime) |
| Public items, errors, versions | [Public API and stability](engineering.md#public-api-and-stability) |
| Module layout, locks, lints | [Code style](engineering.md#code-style) |
| Tests | [Tests](engineering.md#tests) |
| AI, prompts, tools | [AI features](engineering.md#ai-features) |
| Dependencies, CI, release | [Dependencies and supply chain](engineering.md#dependencies-and-supply-chain), then `release` |
| Docs and rustdoc | [Documentation](engineering.md#documentation-and-consumer-experience), then `docs` |
| Which rules are gated | [Validation](engineering.md#validation) and [Known gaps](engineering.md#known-gaps) |

Read the needed section, not the whole file. A test double proves a contract, not real integration behavior.
