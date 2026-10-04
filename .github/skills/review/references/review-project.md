# Review project

Standing, re-runnable **whole-toolkit audit**, independent of any diff. Use it periodically, before a release, when onboarding to a crate, or whenever you want assurance the tree as a whole still honors the baseline. It sequences the same eight focused passes in [`references/`](./) but over the existing code rather than a change set.

## Execution

Follow [the review skill](../SKILL.md): direct review by default; independent agents only on request. Read current source and relevant contracts. A plan is a scope checklist, not a justification for a baseline violation.

## Scope first to keep the audit manageable

The whole workspace is large (70+ publishable crates). Prefer auditing **one workspace or domain at a time** rather than everything at once:

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

## Passes

Follow the trigger table and order in [the review skill](../SKILL.md). Use each checklist's project scope. Load applicable files only; report incomplete checks and stop acceptance on structural/reuse blockers.

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
