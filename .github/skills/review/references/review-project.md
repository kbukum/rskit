# Review project

Standing, re-runnable **whole-toolkit audit**, independent of any diff. Use it periodically, before a release, when onboarding to a crate, or whenever you want assurance the tree as a whole still honors the baseline. It sequences the same eight focused passes in [`references/`](./) but over the existing code rather than a change set.

## Run this in a separate, clean-context agent

**Always dispatch this audit to a fresh agent with no shared session context.** The point of a full audit is an independent read of the code as it exists — not filtered through whatever a prior session believed about it. Do not run it inline in a session that has been editing the same code.

- Hand the agent: the crate(s)/domain to audit (or "the whole workspace"), this file, and the [`references/`](./) folder.
- The agent judges the code as written, against the principles in [`.github/copilot-instructions.md`](../../../copilot-instructions.md) and [`docs/DESIGN.md`](../../../../docs/DESIGN.md) — not against any session's recollection.
- **Optional roadmap check.** If there is a roadmap, ADR, or versioning plan (e.g. `docs/adr/`, `docs/VERSIONING-ROADMAP.md`), pass it in *as context for intended state only* — "here is where the toolkit is meant to be; flag where the tree has not caught up." It frames expectations; it never excuses a baseline violation.

## Scope first to keep the audit manageable

The whole workspace is large (50+ crates). Prefer auditing **one workspace or domain at a time** rather than everything at once:

- a single core crate or domain (`core/rskit-<name>`, `contrib/<domain>/`),
- a whole workspace (`W=core`, `W=contrib`, `W=examples`), or
- the full tree only when you have time for the slow gates.

State the chosen scope up front so findings are bounded.

## Pass 0 — Scope and context

- Initialize tooling if needed (`make setup`).
- Get a structural picture before diving in: list crates and their dependency edges, skim each `src/` tree.

```bash
ls core contrib examples
for c in core/rskit-*/Cargo.toml contrib/*/*/Cargo.toml; do echo "== $c =="; rg '^rskit-' "$c"; done
```

## Passes — run in order

Work the focused files top to bottom; each carries a "Project mode" scope note describing how to sweep the tree for that pass.

1. [`00-structure-placement.md`](./00-structure-placement.md) — crate placement, acyclic layering, facade discipline, new-crate wiring across every workspace.
2. [`01-canonical-reuse.md`](./01-canonical-reuse.md) — sweep `core/` and `contrib/` for local forks of an owned concern. *(blocker class)*
3. [`02-principles.md`](./02-principles.md) — typed/minimal, errors & resilience, concurrency, composition, current idioms, AI features across the full scope.
4. [`03-security-privacy.md`](./03-security-privacy.md) — trust-boundary validation, injection safety, token handling, crypto, data minimization.
5. [`04-quality.md`](./04-quality.md) — root-cause over patches, dead code, outdated patterns, style gates.
6. [`05-tests-tdd.md`](./05-tests-tdd.md) — coverage of behavior and failure paths, determinism, time/env-var discipline, fixtures.
7. [`06-docs-supply-chain.md`](./06-docs-supply-chain.md) — `///` docs, Conventional Commits, `Cargo.lock`, `cargo-deny`, SHA-pinned actions, SBOM/provenance.
8. [`07-comments-rustdoc.md`](./07-comments-rustdoc.md) — sweep all source prose: comments and `///` docs describe the current code, not plans/history; rewrite or delete the rest.

When you only need one pass across the project (e.g. a standalone security or TDD sweep), run that focused file directly with its "Project mode" note.

## Findings

Record every finding as:

```
severity (blocker / should-fix / nit) — file:line — what's wrong — which principle — suggested fix
```

Group findings by crate and by pass so the report is actionable. See [`SKILL.md`](../SKILL.md) for severity definitions.

## Validation

A full audit is the place for the slow, complete gates (scope to a workspace with `W=` when you can):

```bash
make fmt-check
make lint                 # whole-workspace clippy -D warnings (or W=<workspace>)
make build                # or W=core|contrib|examples
make test                 # or W=<workspace>
make doc
make deny                 # cargo-deny + L7-edges + workspace-dep-sync + topology + public-api
make release-coverage     # per-package coverage gate
make check                # full canonical gate
make release-readiness    # supply-chain + API sweep, before a release
```

A green `make check` is necessary but **not sufficient** — unbounded concurrency, missing timeouts/cancellation, global-registry composition issues, duplicated owners, and boundary-validation gaps are on the reviewer, not the gate.
