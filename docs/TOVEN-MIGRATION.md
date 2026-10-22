# Toven migration status

[Toven](https://github.com/kbukum/toven) is rskit's argv-first task/release orchestrator. This document tracks how much of rskit's development and CI surface is driven by Toven, records the exact command mappings for what remains, and enumerates the Toven features that still block full parity.

## Adoption at a glance

| Area | Driver | Status |
|---|---|---|
| Release (plan, status, readiness, bump, tag, publish, sbom, depgraphs) | `toven release …` | Migrated |
| Read-only canary (modules, graph, release previews) | `toven` | Migrated |
| Structural guardrail (`structure` / declare-only aggregators) | `toven run structure` | Migrated |
| Architectural guardrails (`topology`, `l7-edges`, `workspace-deps-sync`, `public-api`) | `toven run <task>` | Migrated (Toven orchestrates; `scripts/rskit_tool.py` still owns the rule logic) |
| Crowded-module advisory | `toven run crowded-modules` | Migrated |
| Release-readiness guardrail sweep | `toven run readiness` | Migrated |
| Module groups (layers) | `toven.toml` `[groups.*]`, `toven groups`, `--group`, `make check-<group>` | Migrated — groups replace `domains.toml`; `guardrails.allow_groups` enforces the layering |
| Module index generation | `toven run module-index` | Migrated — rendered from `toven groups --output jsonl` (see gap 7) |
| Supply-chain vuln scan | `toven vuln` | Task declared |
| build / check / test / lint / fmt / doc / doctest / coverage | `toven <task>` | Migrated locally — the Makefile runs every gate through Toven (`C=`/`W=`/`G=` select modules); **CI still on `scripts/rskit_tool.py`** |
| Affected/changed test + lint matrix in CI | `scripts/rskit_tool.py ci …` | Pending — see mappings and gaps below |
| MSRV compile matrix | `scripts/rskit_tool.py ci msrv` | Pending — see gap 3 |
| Coverage gate | `toven coverage` (single combined run) | Migrated — one whole-workspace invocation gates every crate (security ≥95, per-module floors); resolved the core↔contrib facade-cycle co-scheduling + fuzz no-op fan-out gap. Legacy `scripts/rskit_tool.py coverage` loop removed. |
| Change → affected set | `toven affected` / `toven <task> --base B --merge-base` | Migrated — the rskit domain matrix and its unused CI `changes` output are removed |
| `cargo-deny` (three per-workspace configs) | native `cargo deny` in `make deny` | Blocked — see gap 4 |

Every rskit operation is now runnable through Toven (`toven tasks` lists them). The Makefile guardrail and structure recipes and the CI `guardrails`/`security` jobs are migrated and validated locally. The remaining CI matrix jobs are release-gating and are staged for a follow-up that can exercise them on a branch, tracked by the gaps below.

## Command mappings for the pending CI jobs

These are directly expressible with the current Toven task surface; the migration is staged only because the jobs gate merges and must be verified against a live CI run.

| Current CI step | Toven equivalent |
|---|---|
| `rskit_tool.py ci lint --scope changed --changed-base B --feature-mode all` | `toven lint --base B --merge-base` |
| `rskit_tool.py ci lint --scope all --feature-mode all` | `toven lint` |
| `rskit_tool.py ci test --scope changed --changed-base B --feature-mode all --profile ci` | `toven test --base B --merge-base -- --profile ci` |
| `rskit_tool.py ci test --scope all --feature-mode default --profile ci` | `toven run test-default -- --profile ci` |
| `rskit_tool.py ci msrv --scope all --feature-mode all` | `toven check -- --all-features` (run inside the 1.97-toolchain job; see gap 3) |
| `rskit_tool.py coverage --mode coverage --changed --changed-base B --line-threshold 70` | `toven coverage --base B --merge-base --line 70` |
| `rskit_tool.py coverage --mode coverage --clean full --line-threshold 90` | `toven coverage --line 90` (cache/artifact `--clean` has no Toven analog; see gap 5) |

The `lint`, `test` and `doctest` tasks always build with `--all-features`, matching the local gates. `test-default` is a separate task for the default-features leg; `make test-affected` runs both. Lint and MSRV default-features legs stay on `scripts/rskit_tool.py` until task variants land (gap 2).

## Toven feature gaps blocking full parity

Prioritized list of Toven capabilities rskit needs before the pending rows above can be fully retired. File these upstream in the Toven repo.

### 1. Named module groups — resolved

rskit's layers are `[groups.*]` in `toven.toml`; `domains.toml` is gone. `toven groups` lists them, `--group` selects them (`make check-<group>`), and `guardrails.allow_groups` rejects any dependency that reaches up a layer. The module index and layer diagram read `toven groups --output jsonl`. The old rskit domain matrix is dropped: Toven's changed-plus-dependents selection replaces it.

### 2. Feature-set matrix / task variants — medium priority

rskit validates each gate under multiple feature sets (`default` and `--all-features`), and MSRV under both. Today each leg is a separate invocation with different passthrough. Toven has no first-class "run this task across a declared feature matrix" with per-variant caching.

**Requested:** declarable task variants / a feature matrix so one `toven test` plans and caches the default and all-features legs as distinct units. In the interim, rskit declares `test-default` as a second task beside `test`; lint and MSRV keep the CI matrix + passthrough.

### 3. Per-task toolchain selection (MSRV) — low priority

rskit's MSRV job compiles on Rust 1.97. This works under Toven by activating 1.97 in the job and letting Toven use the ambient `cargo`, but there is no Toven-native "run task X on toolchain 1.97" knob.

**Requested:** optional per-task/per-run toolchain pin (e.g. `toolchain = "1.97"` or `--toolchain 1.97`).

### 4. Per-workspace task argv / config overrides (cargo-deny) — medium priority

rskit ships three `cargo-deny` policies — `deny.toml` (core), `deny.contrib.toml` (contrib), `deny.examples.toml` (examples). A single whole-workspace Toven task template cannot map each workspace to its irregularly named config, so `make deny` stays native.

**Requested:** per-workspace argv/config overrides for a task, or a `{workspace.name}` template plus an explicit workspace→config mapping.

### 5. Coverage: cache/artifact reset — low priority

rskit's coverage supports `--clean full` (reset coverage artifacts before a run) and a security-scoped line threshold. The security subset already maps cleanly onto Toven coverage **profiles** (`[ecosystems.rust.coverage.profiles.security]`), so only the `--clean` artifact-reset behavior is missing.

**Requested:** an optional coverage artifact-reset flag, or documentation of the intended clean-run workflow.

### 6. Toven-native dependency & manifest policy checks — medium priority

Some guardrails are wired as `command`-ecosystem tasks today only because rskit implements them in Python — but they are **generic graph/manifest checks, not rskit policy**. Toven already owns the dependency graph and every module's manifest, so it has the *mechanism*; only the *data* (which edges are forbidden) is rskit-specific and belongs in config. These are the strongest candidates to become Toven-native, config-driven checks so the Python wrappers can retire:

- **Forbidden dependency edges** (today: `l7-edges`). rskit checks a hardcoded list of disallowed crate→crate edges by shelling out to `cargo tree`. Group guardrails cover layer-to-layer direction but not these per-crate rules, which apply to normal dependencies only, so `l7-edges` stays an rskit check for now. A config-driven per-crate edge list in Toven could retire it later.
- **Shared dependency-version consistency** (today: `workspace-deps-sync`). Verifying that shared external dependency versions and the workspace package version are aligned across `core`/`contrib`/`examples` is generic to any multi-workspace Cargo repo. Toven discovers all three manifests already.

**Requested:** a config-driven Rust-adapter policy surface for (a) forbidden/required dependency edges and (b) cross-workspace dependency-version consistency.

Lower-priority relatives, for completeness: a public-API-diff capability in the Rust adapter (today: `public-api` via `cargo-public-api`) and a structural file-count advisory (today: `crowded-modules`). Both are generic but lower value.

**Stays rskit regardless.** `topology` (facade-only contrib aggregation, `rskit-grpc` ≠ `rskit-server`, `rskit-util` L0 purity, heavy transport deps must be `optional`, removed-crate guards) and `readiness` (SHA-pinned Actions, no runtime panic/unwrap, forbid-unsafe-without-`SAFETY`, required fuzz-target set) encode rskit-specific architecture and code hazards. Toven should orchestrate these as tasks, not reimplement them — unless it grows a general declarative dependency-policy DSL, at which point most of `topology` could move to config too.

### 7. Doc/report projection hook (module index) — low priority

`toven run module-index` shells out to `scripts/rskit_tool.py` to regenerate `docs/MODULE-INDEX.md` from `toven groups --output jsonl`. A native Toven "render a repository doc from module metadata" projection would let this script retire; deferred until a second user needs it.

## What is explicitly not a gap

- **Manifest resolution.** Toven resolves each module's manifest and package selector natively, so the old `rskit_tool ci package-manifest` helper is obsolete under Toven.
- **Guardrail rule logic.** `topology` and `readiness` encode rskit-specific architecture and code hazards; Toven correctly orchestrates them as `command`-ecosystem tasks and should not reimplement the rules. Note the nuance in gap 6: `l7-edges` and `workspace-deps-sync` are *generic* graph/manifest checks that Toven could own natively — they are wrapped as tasks today only because rskit implements them in Python.
- **Affected planning, waves, caching, watch, jobs.** Fully supported by Toven today.
