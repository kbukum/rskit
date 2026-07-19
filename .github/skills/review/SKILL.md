---
name: review
description: >-
    Run rskit's standing engineering-baseline review over a change set (a branch, commit range,
    or HEAD~1) or over a whole crate/domain/tree. Sequences eight focused passes — structure &
    placement, canonical reuse, principles, security & privacy, quality, tests/TDD, docs & supply
    chain, comments & rustdoc. Use before merging a change, when auditing a crate, or before a
    release. Always run it in a fresh, clean-context reviewer.
---

# Reviewing rskit against its engineering baseline

rskit is shared foundation infrastructure and the **reference kit** that gokit and pykit mirror: a defect in a core crate propagates to the `rskit` facade, the other core crates, every `contrib/` adapter, and every downstream consumer (the parity kits, Toven, and services that depend on rskit). The standard is correspondingly high — security, concurrency, and composition each get their own pass. This skill encodes rskit's permanent review baseline as eight focused passes plus three orchestrators.

The authoritative baseline lives in [`.github/copilot-instructions.md`](../../copilot-instructions.md) (and `docs/DESIGN.md`). A plan, spec, issue, or roadmap (e.g. an ADR under `docs/adr/`) may be passed in **as a scope checklist only** — it defines intended scope, never excuses a baseline violation. If the code diverges from the plan, report the divergence; the baseline wins.

## Run in a separate, clean-context agent

**Always dispatch a review to a fresh reviewer with no shared session context** — never inline in the session that wrote the code. A reviewer that "remembers" writing the change rationalizes it; an independent agent re-derives every judgment from the code and the principles. Hand it only the scope (diff or crate/domain) and this skill.

## Pick a driver

- **Change set** → [`references/review-changes.md`](references/review-changes.md). A diff (branch, commit range, or `HEAD~1`). Use after every change set, especially fast AI-assisted work.
- **Whole tree / crate** → [`references/review-project.md`](references/review-project.md). A standing audit independent of any diff. Use periodically, before a release, or when onboarding.
- **Review → fix in one pass** → [`references/review-details.md`](references/review-details.md). Splits the review into parallel subagent passes by Rust concern, then plans and applies fixes.

## The eight focused passes (run in order)

Stop and reject as soon as a change fails pass `00` or `01` — misplaced or duplicated code makes every later pass unreliable. Each file also carries a "Project mode" note for tree-wide sweeps and can be run standalone when you need only one pass.

1. [`references/00-structure-placement.md`](references/00-structure-placement.md) — crate placement (`core`/`contrib`/`examples`), acyclic layering, facade discipline, new-crate wiring.
2. [`references/01-canonical-reuse.md`](references/01-canonical-reuse.md) — did the code reimplement a concern an existing core crate (or std) already owns? *(blocker class)*
3. [`references/02-principles.md`](references/02-principles.md) — typed/minimal APIs, errors & resilience, concurrency, composition, current idioms, AI/model features.
4. [`references/03-security-privacy.md`](references/03-security-privacy.md) — trust-boundary validation, injection safety, token handling, crypto, data minimization.
5. [`references/04-quality.md`](references/04-quality.md) — root-cause over patches, dead code, maintainability, style gates.
6. [`references/05-tests-tdd.md`](references/05-tests-tdd.md) — TDD, determinism under race/shuffle/parallel, `tokio::time` and env-var discipline, fixtures.
7. [`references/06-docs-supply-chain.md`](references/06-docs-supply-chain.md) — `///` docs, Conventional Commits, `Cargo.lock`, `cargo-deny`, SHA-pinned actions, SBOM/provenance.
8. [`references/07-comments-rustdoc.md`](references/07-comments-rustdoc.md) — comments and `///` docs describe the code as it is, not plans/history/process.

## Severity and finding format

```
severity (blocker / should-fix / nit) — file:line — what's wrong — which principle — suggested fix
```

- **blocker** — hard-principle violation (upward/cyclic dependency, concern reimplemented, panic on a runtime path, `unwrap`/`expect`/swallowed error on a fallible runtime path, unbounded channel / task with no cancellation, global mutable registry / import-time side effect, trust boundary not validated, `unsafe` without `// SAFETY:`, behavioral change with no test). Fix before merge.
- **should-fix** — real defect or debt that isn't a baseline violation (compat shim, `std::thread::sleep` in a test, env-var test without the guard, inline config instead of a fixture, reinvented std facility, one large file that should be split by concern).
- **nit** — minor/style, take-it-or-leave-it.

## Validation is via make/cargo (see the `validate` skill)

**Scope every command to the changed crate(s)** — the full-workspace gates are slow across 50+ crates and belong to a project audit or CI sign-off, not a per-change review:

```bash
make fmt-check                       # fast, whole-tree formatting check
make lint C=<crate>                  # clippy, scoped to the crate
make test C=<crate> T=<pattern>      # scoped tests
make test-affected                   # only crates the diff touches
make check-topology                  # cheap placement/acyclicity guard
make check-public-api                # only if a public surface changed
make lint W=core                     # W=core|contrib|examples — one workspace
make check                           # full canonical gate — audit/CI sign-off
make deny                            # cargo-deny + workspace-dep-sync + topology + public-api
```

Treat a green run as **necessary but not sufficient**: it does not catch unbounded concurrency, missing timeouts/cancellation, global-registry composition issues, duplicated owners, or boundary-validation gaps. Those are on the reviewer.
