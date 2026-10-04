# Review changes

Standing, re-runnable review of a **change set** in this repository — a branch, a commit range, or `HEAD~1`. Use it after every change set, especially fast AI-assisted work. It sequences the eight focused passes in [`references/`](./) over a diff and adds scope handling; the actual checks live in the focused files.

## Execution

Follow [the review skill](../SKILL.md): direct review by default; independent agents only on request. Read current source and relevant contracts. A plan is a scope checklist, not a justification for a baseline violation.

## Pass 0 — Scope and context

- Get the actual diff: `git diff <base>...HEAD --stat`, then per file. Review what changed **plus its blast radius** — the rest of each touched file, the code the change calls and is called by, and closely-related files in the same crate. Do not audit the whole repo (that is [`review-project.md`](./review-project.md)), but do not tunnel-vision on the diff lines either.
- **Pre-existing problems in the blast radius are in scope.** A defect, dead code, duplicated concern, or design smell you read while reviewing is reported like any other finding — the change set is not a shield for the code around it. Because rskit is pre-stable with **no backward compatibility owed**, prefer a root-cause **redesign** over patching the symptom (decide Redesign / Align / Enhance / Drop; "leave it patched" is not an option). Flag when a fix reaches beyond the touched files and keep it coherent; never silently refactor unrelated code.
- rskit is a foundation toolkit: a change to a core crate's public surface affects the facade, other core crates, `contrib/` adapters, and downstream repos (gokit parity, Toven). List the affected area before reviewing.
- Note whether the change belongs in `core/`, `contrib/`, or `examples/`, and whether it belongs in *this* crate at all.

## Passes

Follow the trigger table and order in [the review skill](../SKILL.md). Use each checklist's changes scope. Load applicable files only; report incomplete checks and stop acceptance on structural/reuse blockers.

## Findings

Record every finding as:

```
severity (blocker / should-fix / nit) — file:line — what's wrong — which principle — suggested fix
```

See [`SKILL.md`](../SKILL.md) for severity definitions.

## Validation

**Scope every command to the changed crate(s) — do not run the full workspace gates here.** rskit has 70+ publishable crates; `make check` / `make test` / `make build` across the whole workspace are slow and are reserved for [`review-project.md`](./review-project.md) or final pre-merge sign-off (typically in CI). For a change set, run only:

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
