# Review and apply fixes

Use only when fixes are requested. Follow [review](../SKILL.md) and the [baseline](../../../copilot-instructions.md). Preserve edits/index; review directly unless an independent agent is requested.

## Scope

Inspect the requested diff or tree, including close callers/callees and contracts. Flag workspace manifests, lints, shared errors, facade exports, adapter impacts, and downstream consumers. Narrative explains scope, never excuses violations.

## Passes

Use [validate](../../validate/SKILL.md) for current scoped gates. Load detailed rules by trigger; prose-only work needs claim/link/metadata checks, not runtime suites.

| Pass | Trigger and checks | Detail |
|---|---|---|
| Placement/reuse | Crate/module changes or shared behavior; downward imports, declare-only aggregators, canonical owners | [00](00-structure-placement.md), [01](01-canonical-reuse.md) |
| Correctness | Rust behavior; ownership, cause-preserving errors, no fallible runtime unwrap/expect/success fallback, safe cleanup and non-panicking Drop | [02](02-principles.md), [04](04-quality.md) |
| Concurrency | Async/threaded work; bounded owned tasks/drain, no lock/borrow across await or nested runtime blocking, safe Send/Sync, paused test time | [02](02-principles.md), [05](05-tests-tdd.md) |
| Composition | Components/factories/registries; injected policy, typestate ordering, feature-isolated adapters, no global registry or import-time I/O | [00](00-structure-placement.md), [02](02-principles.md) |
| Security/config | Untrusted data, auth, paths, platform behavior; validated bounds, SQL parameters, argv-only processes, header credentials, redaction, config precedence, exercised platform branches | [03](03-security-privacy.md) |
| API/dependencies | Public items/manifests; narrow visibility, typed APIs, borrowed inputs where ownership is unnecessary, growing non-exhaustive types, must-use builders, workspace lints/metadata, maintained synchronized dependencies and locks | [01](01-canonical-reuse.md), [04](04-quality.md) |
| Tests/docs | Every scope; deterministic failures/regressions, coverage, compiled examples, accurate rustdoc and claims | [05](05-tests-tdd.md), [06](06-docs-supply-chain.md), [07](07-comments-rustdoc.md) |

Apply the repository's unsafe policy; allowed unsafe requires substantive safety justification and valid edition syntax. Serialize environment changes and use `CurrentDirGuard` for cwd mutation. Shared doubles do not replace real adapter/process/TLS proof. Check public-API, topology, dependency synchronization, documentation, and coverage gates when relevant.

## Findings and acceptance

Report `severity — file:line — finding — principle — fix`, grouping patterns. Principle violations, wrong behavior, and broken contracts block acceptance. Explain skipped/deferred passes and incomplete evidence.

For authorized fixes, choose Redesign/Align/Enhance/Drop and implement test-first. Ask only for unresolved scope/behavior decisions. Run affected gates and required acceptance; commit/amend/push/PR require separate authorization.
