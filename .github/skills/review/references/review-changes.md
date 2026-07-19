# Review changes

Standing, re-runnable review of a **change set** in this repository — a branch, a commit range, or `HEAD~1`. Use it after every change set, especially fast AI-assisted work. It sequences the eight focused passes in [`references/`](./) over a diff and adds scope handling; the actual checks live in the focused files.

## Run this in a separate, clean-context agent

**Always dispatch this review to a fresh reviewer agent with no shared session context.** A reviewer that "remembers" writing the code rationalizes it; an independent agent re-derives every judgment from the diff and the principles. Do not run it inline in the same session that produced the change.

- Hand the reviewer agent: the diff (or base ref), this file, and the [`references/`](./) folder. Nothing else from the authoring session.
- The reviewer reads the code as-is; it does not trust prior reasoning about why the code "should" be correct.
- **Optional plan check.** If a plan/spec/ADR exists (e.g. a `docs/adr/` entry, an issue, or a design doc), pass it in *as a scope checklist only* — "here is what this change set claimed to do; verify the diff actually did it, with tests." The plan defines intended scope; it never excuses a principle violation. If the diff diverges from the plan, report the divergence; the baseline in [`.github/copilot-instructions.md`](../../../copilot-instructions.md) wins over any plan.

## Pass 0 — Scope and context

- Get the actual diff: `git diff <base>...HEAD --stat`, then per file. Review only what changed plus its affected area; do not audit the whole repo (that is [`review-project.md`](./review-project.md)).
- rskit is a foundation toolkit: a change to a core crate's public surface affects the facade, other core crates, `contrib/` adapters, and downstream repos (pykit/gokit parity, Toven). List the affected area before reviewing.
- Note whether the change belongs in `core/`, `contrib/`, or `examples/`, and whether it belongs in *this* crate at all.

## Passes — run in order, stop early on a structural failure

Work the focused files top to bottom. **Stop and reject as soon as a change fails pass `00` or `01`** — misplaced or duplicated code makes every later pass unreliable.

1. [`00-structure-placement.md`](./00-structure-placement.md) — crate placement, acyclic layering, facade discipline, new-crate wiring.
2. [`01-canonical-reuse.md`](./01-canonical-reuse.md) — reuse vs. reimplementation of a core-crate/std-owned concern. *(blocker class)*
3. [`02-principles.md`](./02-principles.md) — typed/minimal APIs, errors & resilience, concurrency, composition, current idioms, AI features.
4. [`03-security-privacy.md`](./03-security-privacy.md) — trust-boundary validation, injection safety, token handling, crypto, data minimization.
5. [`04-quality.md`](./04-quality.md) — root-cause over patches, dead code, maintainability, style gates.
6. [`05-tests-tdd.md`](./05-tests-tdd.md) — TDD, race/shuffle/parallel determinism, time/env-var discipline, fixtures.
7. [`06-docs-supply-chain.md`](./06-docs-supply-chain.md) — `///` docs, Conventional Commits, `Cargo.lock`, `cargo-deny`, SHA-pinned actions, SBOM.
8. [`07-comments-rustdoc.md`](./07-comments-rustdoc.md) — comments and `///` docs explain the code as it is; rewrite or delete plan/history/process prose.

Each focused file carries a "Changes mode" scope note — follow that mode here. When you only need one pass (e.g. just security, just TDD), run that focused file directly instead of this orchestrator.

## Findings

Record every finding as:

```
severity (blocker / should-fix / nit) — file:line — what's wrong — which principle — suggested fix
```

See [`SKILL.md`](../SKILL.md) for severity definitions.

## Validation

**Scope every command to the changed crate(s) — do not run the full workspace gates here.** rskit has 50+ crates; `make check` / `make test` / `make build` across the whole workspace are slow and are reserved for [`review-project.md`](./review-project.md) or final pre-merge sign-off (typically in CI). For a change set, run only:

```bash
make fmt-check                       # fast, whole-tree formatting check
make lint C=<crate>                  # clippy, scoped to the crate
make build C=<crate>
make test C=<crate> T=<pattern>      # narrow further with a test pattern
make test-affected                   # or: make coverage-changed — only crates the diff touches
make check-topology                  # fast placement/acyclicity guard (cheap; run if structure changed)
make check-public-api                # only if a public surface changed
make doc C=<crate>                   # only if public docs changed
```

When a change spans a whole workspace or domain rather than a single crate, scope to that level — still much cheaper than the global gate:

```bash
make lint W=core                     # W=core|contrib|examples — one workspace
make test W=contrib
make check-core                      # per-domain gate: check-core|check-data|check-transport|check-auth|
                                     #   check-ai|check-media|check-infra|check-crosscutting|check-composition|...
```

Prefer `make test-affected` / `make coverage-changed` over the unscoped targets — they run only the crates impacted by the current changes. Step up to `W=<workspace>` or a per-domain `make check-<domain>` when the change spans a workspace/domain. Run the full `make check` / `make deny` only when the change is genuinely workspace-wide, or leave it to CI for sign-off. A green scoped run is necessary but **not sufficient** — it will not catch unbounded concurrency, missing timeouts/cancellation, global-registry composition issues, duplicated owners, or boundary-validation gaps. Those are on the reviewer.
