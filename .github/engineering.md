# rskit engineering standard

The rules every rskit change follows. Read only the section your task needs. Agents start at the [agent entry point](copilot-instructions.md).

rskit is a pre-1.0 Rust infrastructure toolkit. It is a sibling of [gokit](https://github.com/kbukum/gokit) (Go). Neither kit is canonical: idiomatic Rust comes first, then rskit consistency, then parity of wire contracts and concepts. See [sibling-parity](skills/sibling-parity/SKILL.md).

## Read when

| You are | Read |
|---|---|
| Adding a crate, adapter, or shared helper | [Placement and ownership](#placement-and-ownership) |
| Handling input, secrets, I/O, tasks, or `unsafe` | [Security and runtime](#security-and-runtime) |
| Adding or changing a public item | [Public API and stability](#public-api-and-stability) |
| Writing Rust code | [Code style](#code-style) |
| Writing or fixing tests | [Tests](#tests) |
| Adding model, prompt, or tool features | [AI features](#ai-features) |
| Adding a dependency or touching CI or release | [Dependencies and supply chain](#dependencies-and-supply-chain) |
| Writing docs, rustdoc, or comments | [Documentation and consumer experience](#documentation-and-consumer-experience) |
| Finishing a change | [Validation](#validation) and [Definition of Done](#definition-of-done) |

## Where guidance lives

Each topic has one owner. Other files link to it.

| Topic | Owner |
|---|---|
| What rskit is, packages, quickstart, status | [README](../README.md), [package catalog](../docs/PACKAGES.md) |
| Where code goes: layers, owners, facade | [Design notes](../docs/DESIGN.md), [concern owners](../docs/CONCERN-OWNERS.md), [naming and structure](../docs/naming-and-structure.md), [test layout](../docs/TEST-LAYOUT.md) |
| Why a decision was made | [ADRs](../docs/adr/) |
| Rules every change follows | This file |
| Stability, versions, deprecation | [SemVer policy](../docs/policy/SEMVER.md), [deprecation policy](../docs/policy/DEPRECATION.md), [versioning](../docs/VERSIONING.md) |
| Contributor workflow and commit style | [CONTRIBUTING](../CONTRIBUTING.md) |
| Vulnerability reporting and secure use | [SECURITY](../SECURITY.md), [security model](../docs/security-model.md) |
| Agent startup and routing | [Agent entry point](copilot-instructions.md) |
| Procedures | [Skills](skills/README.md) |
| Gates | `make help`, [`Makefile`](../Makefile), [`toven.toml`](../toven.toml), [CI workflow](workflows/ci.yml) |

## Principles

- **Pre-stable redesign.** Fix root causes with Redesign, Align, Enhance, or Drop. No compatibility shims. Update dependent callers in the same change. Keep sound code.
- **Simplest complete design.** Use current idiomatic Rust. Complexity must earn its place.
- **Consistency over parity.** Project-wide consistency and idiomatic Rust outrank matching gokit symbols. Match wire contracts and behavior, not another language's internal shapes.
- **Keep code current.** Prefer the standard library. Check that a dependency is maintained and has no open advisory.

## Placement and ownership

- Code goes in the lowest layer that owns the concern. Layers are the `[groups.*]` in [`toven.toml`](../toven.toml); see [design notes](../docs/DESIGN.md#group-layers) and [ADR 0001](../docs/adr/0001-layered-crate-architecture.md).
- Imports point down only. When a lower layer needs a higher service, it declares a trait and the higher layer implements it.
- Before writing shared logic, find its owner in [concern owners](../docs/CONCERN-OWNERS.md). Consume it or enhance it; never fork it.
- `core/rskit-*` holds foundations and the facade. `contrib/<domain>/<name>` holds adapters. `examples/` holds consumers. Each is its own Cargo workspace; there is no root `Cargo.toml`.
- Adapters are opt-in. Config selects them through explicit, injected registries. No import-time I/O and no global mutable registries. Inject telemetry, clients, and policies.
- Provider shapes are `RequestResponse`, `Stream`, `Sink`, and `Duplex`.
- Naming and file layout follow [naming and structure](../docs/naming-and-structure.md).

## Security and runtime

- Validate input at every trust boundary. Secure defaults and least privilege.
- No secrets in code, logs, errors, or URLs. Tokens go in headers. Redact and bound retention of sensitive data.
- Parameterized queries only. Subprocesses use argv, never a shell string. Use current crypto from [`rskit-encryption`](../core/rskit-encryption/) and [`rskit-security`](../core/rskit-security/).
- Paths confined to a root use `rskit-fs` `confine_path`, which also rejects symlink escapes.
- Every remote call has a timeout. Retry only idempotent operations, with bounded jittered backoff. Degrade explicitly.
- Every task has an owner, cancellation, and shutdown. Bound queues, buffers, and concurrency, and document backpressure. Drain on shutdown.
- No runtime `unwrap`, `expect`, or `panic!`. No swallowed errors and no success-shaped fallbacks. Tests may unwrap.
- `unsafe` is forbidden by the workspace lints. Code that needs FFI documents every block with a `// SAFETY:` comment. See [Known gaps](#known-gaps).
- The threat model and production defaults live in the [security model](../docs/security-model.md).

## Public API and stability

- Keep public APIs typed and minimal. No `Box<dyn Any>` or stringly typed escape hatches in public signatures.
- Return `AppResult<T>` and build `AppError` with an `ErrorCode`. Preserve the cause. Errors tell the caller what failed and what to do. gRPC status mapping belongs in `rskit-grpc`.
- Mark public enums and structs that may grow `#[non_exhaustive]`.
- Use builders or parameter structs for related or same-typed inputs. Mark `with_*` builder methods `#[must_use]`.
- Hide implementation types. Prefer `pub(crate)`. Re-export the call-site API from the crate root.
- Every crate has its own version. Pre-1.0 releases may break; record every break under `Changed` with a **Breaking** prefix in [CHANGELOG](../CHANGELOG.md). The [SemVer policy](../docs/policy/SEMVER.md) and [deprecation policy](../docs/policy/DEPRECATION.md) own the details.
- Facade features expose crates. Keep defaults small so users adopt one crate at a time.

## Code style

- `rustfmt.toml` and `clippy.toml` own formatting and lint settings. Crates inherit the workspace lints with `[lints] workspace = true`.
- `lib.rs` and `mod.rs` hold declarations and re-exports only. Split modules by responsibility, not line count. Details in [naming and structure](../docs/naming-and-structure.md).
- Use `parking_lot::Mutex`, not `std::sync::Mutex`. Use an async lock only when the guard must cross `.await`. Never hold a lock across unrelated I/O.
- Use `CancellationToken` for shutdown, not `std::process::exit`.
- Document every public item.

## Tests

- Test first: a failing test, the smallest fix, then refactor while green. Every fix gets a regression test.
- Test behavior, including failure paths. Tests are deterministic and pass in any order and in parallel: use `tokio::time::pause()` and `advance()`, never sleep. No real network in default runs.
- Serialize environment mutation. Tests that read or change the working directory use `rskit_testutil::CurrentDirGuard`.
- Reuse doubles from `rskit-testutil`; add new ones there. A double proves a contract, not real integration.
- Test placement follows [test layout](../docs/TEST-LAYOUT.md). Coverage floors live in [`toven.toml`](../toven.toml) `[ecosystems.rust.coverage]`; do not copy the numbers.

## AI features

- Model output and retrieved context are untrusted input. Validate structured output.
- Tool calls get least privilege. Destructive actions need a human gate.
- Version prompts with `rskit-ai` `PromptTemplate` and gate prompt or model changes on evaluations.

## Dependencies and supply chain

- Add shared dependencies through the owning workspace manifest. Keep versions in sync across workspaces.
- New dependencies must be maintained, licensed under the allowed list in `deny.toml`, and free of advisories.
- CI actions are pinned by commit SHA. Releases ship signed artifacts and SBOMs. See [releasing](../docs/RELEASING.md).

## Documentation and consumer experience

- Lead with the reader's task, then the shortest working example. Remove repetition and implementation history.
- Write Markdown, rustdoc, and comment prose as natural lines. Never hard-wrap prose to a column. Keep code blocks, lists, tables, and directives intact. `max_width` applies to code only.
- Comments and rustdoc describe the code as it is, not its history or plans.
- Public items have rustdoc with an example when the use is not obvious. Crate READMEs show how to adopt the crate on its own.
- Update the changelog with outcomes, not process. Stable docs never link to `tmp/` plans.

## Validation

Run the smallest gate that covers the change, then the full gate before a PR. `make help` lists every target.

- `make test C=<crate> T=<pattern>`, `make lint C=<crate>`, and `make doc C=<crate>` while working.
- `make check` before a PR. `make deny` for dependency and topology changes. `make coverage-changed` for coverage.
- The [validate skill](skills/validate/SKILL.md) maps changes to gates.

Rules that a gate checks today:

| Rule | Gate |
|---|---|
| Formatting | `make fmt-check` |
| Clippy, workspace lints, `std::sync::Mutex` and `process::exit` bans | `make lint` (inheriting crates only for workspace lints) |
| Declare-only `lib.rs` and `mod.rs` | `make structure` |
| Layer direction | Toven `[groups.*]` `guardrails.allow_groups`, checked when any Toven task plans |
| Specific forbidden edges | `make check-topology`, `make check-l7-edges` |
| Licenses, advisories, sources | `make deny` |
| Shared dependency versions | `make check-workspace-deps-sync` |
| Rustdoc warnings | CI docs job (`RUSTDOCFLAGS=-D warnings`) |
| Coverage floors | `make coverage`, CI coverage job |
| SHA-pinned actions, no runtime `unwrap`/`expect`/`panic!`, `SAFETY:` comments, required fuzz targets | `toven run readiness`, CI security job |

## Known gaps

These rules are written but not fully enforced. They are intentionally not built yet and open to revisit.

- Many crates do not inherit the workspace lints yet, so `unsafe_code = "forbid"`, `missing_docs`, and the pedantic lints do not apply to them.
- `rskit-process` needs `unsafe` for libc calls. The FFI exception is not written into an ADR.
- `make check-public-api` reports public API diffs for `rskit-suite` but does not fail on breaking changes. No `cargo-semver-checks` gate.
- `#[non_exhaustive]`, `#[must_use]` on builders, and test-first are review-only.
- Race, shuffle, and repeated-run test modes are not gated.
- `docs/MODULE-INDEX.md` and dependency graphs are regenerated by hand; no freshness check.
- Crates publish with a registry token, not crates.io trusted publishing.

## Definition of Done

- [ ] Code sits in the right layer and reuses the concern owner.
- [ ] Trust boundaries, bounds, cancellation, and shutdown are handled.
- [ ] Public API is minimal and typed; breaking changes are in the changelog.
- [ ] Tests were written first, cover failure paths, and are deterministic.
- [ ] Rustdoc, READMEs, and docs match the code.
- [ ] Scoped gates pass, and `make check` passes before a PR.
- [ ] No Git action without authorization.
