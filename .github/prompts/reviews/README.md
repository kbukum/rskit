# rskit review prompts

A set of standing, re-runnable review prompts for this repository. They encode rskit's permanent engineering baseline (see [`.github/copilot-instructions.md`](../../copilot-instructions.md) and [`docs/DESIGN.md`](../../../docs/DESIGN.md)) so any change set — or the whole toolkit — can be reviewed the same way every time.

rskit is shared foundation infrastructure: a defect in a core crate propagates to the facade, the other core crates, every `contrib/` adapter, and every downstream consumer (pykit/gokit parity repos, Toven, …). The bar here is correspondingly high — security, concurrency, and composition each get their own lens. Each prompt works as either a human checklist or the instruction block you hand an AI reviewer.

## What is here

Two orchestrators that run the full review:

- [`review-changes.md`](./review-changes.md) — review a diff (a branch, commit, or `HEAD~1`). Use after every change set, especially fast/"vibe-coded" work.
- [`review-project.md`](./review-project.md) — audit the whole tree, independent of any diff. Use periodically, before a release, or when onboarding to a crate.

Seven focused passes, each runnable on its own when you only need one lens:

- [`00-structure-placement.md`](./00-structure-placement.md) — crate placement (`core`/`contrib`/`examples`), acyclic layering, facade discipline, new-crate wiring.
- [`01-canonical-reuse.md`](./01-canonical-reuse.md) — did the code reimplement a concern an existing core crate (or std) already owns?
- [`02-principles.md`](./02-principles.md) — typed/minimal APIs, errors & resilience, concurrency, composition, currency, AI/model features.
- [`03-security-privacy.md`](./03-security-privacy.md) — trust-boundary validation, injection safety, token hygiene, crypto, data minimization.
- [`04-quality.md`](./04-quality.md) — root-cause over patches, dead code, maintainability, style gates.
- [`05-tests-tdd.md`](./05-tests-tdd.md) — TDD, determinism under race/shuffle/parallel, time/env-var test discipline, fixtures.
- [`06-docs-supply-chain.md`](./06-docs-supply-chain.md) — `///` docs, Conventional Commits, `Cargo.lock`, `cargo-deny`, SHA-pinned actions, SBOM/provenance.

The orchestrators sequence these passes and add scope handling; the focused files hold the actual checks. Read the focused file you need and run it directly when a full review is overkill.

## Run reviews in a separate, clean-context agent

Always dispatch a review to a **fresh reviewer agent with no shared session context** — never inline in the session that produced the code. A reviewer that "remembers" writing the change rationalizes it; an independent agent re-derives every judgment from the code and the principles. Hand the agent only the scope (diff or crate/area), the relevant prompt, and this `reviews/` folder.

A plan, spec, issue, or roadmap (e.g. an ADR under `docs/adr/`) may be passed in *as a scope checklist only* — it defines intended scope ("verify the change did what it claimed, with tests") but never excuses a baseline violation. If the code diverges from the plan, report the divergence; the baseline in [`.github/copilot-instructions.md`](../../copilot-instructions.md) wins over any plan.

## How to run any prompt

1. **Pick scope.** Changes review: set a base ref and get the diff (`git diff <base>...HEAD --stat`, then per file). Project review: pick the crate(s)/domain or the whole workspace.
2. **Work passes in order** (00 → 06). Stop and reject as soon as a change fails pass `00` or `01` — misplaced or duplicated code makes every later pass moot.
3. **Run the validation commands** (below). Treat green `make check` as necessary but not sufficient: it does not catch unbounded concurrency, missing timeouts/cancellation, global-registry composition smells, duplicated owners, or boundary-validation gaps. Those are on the reviewer.

## Severity and finding format

Record every finding as:

```
severity (blocker / should-fix / nit) — file:line — what's wrong — which principle — suggested fix
```

- **blocker** — violates a hard principle (upward/cyclic dependency, concern reimplemented, panic on a runtime path, unbounded channel / task with no cancellation, global mutable registry / import-time side effect, trust boundary not validated, `unsafe` without `// SAFETY:`, behavioral change with no test). Must be fixed before merge.
- **should-fix** — real defect or debt that is not a baseline violation (compat shim, `std::thread::sleep` in a test, env-var test without the guard, inline config instead of a fixture, reinvented std facility).
- **nit** — minor/style, take-it-or-leave-it.

## Validation commands

**For a change set, scope every command to the changed crate(s)** — the full workspace gates are slow across 50+ crates and belong to a project audit or CI sign-off, not a per-change review:

```bash
make fmt-check                       # fast, whole-tree formatting check
make lint C=<crate>                  # clippy, scoped to the crate
make test C=<crate> T=<pattern>      # scoped tests
make test-affected                   # or coverage-changed — only crates the diff touches
make check-topology                  # cheap placement/acyclicity guard
make check-public-api                # only if a public surface changed
```

When a change spans a whole workspace or domain (not just one crate), scope to that level instead of the full tree — still far cheaper than the global gate:

```bash
make lint W=core                     # W=core|contrib|examples — one workspace
make test W=contrib
make check-core                      # per-domain gate: check-core|check-data|check-transport|check-auth|
                                     #   check-ai|check-media|check-infra|check-crosscutting|check-composition|...
```

**For a project audit or final sign-off**, run the full gates (optionally scoped to a workspace with `W=core|contrib|examples`):

```bash
make build && make test              # or W=<workspace>
make doc                             # -D warnings
make deny                            # cargo-deny + L7-edges + workspace-dep-sync + topology + public-api
make check                           # full canonical gate
make release-coverage                # per-package coverage gate
make release-readiness               # supply-chain + API sweep before a release
```

Treat green `make check` as necessary but not sufficient: it does not catch unbounded concurrency, missing timeouts/cancellation, global-registry composition smells, duplicated owners, or boundary-validation gaps. Those are on the reviewer.
