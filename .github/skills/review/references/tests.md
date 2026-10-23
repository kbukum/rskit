# Lens: tests and proof

Tests prove behavior. A late, happy-path-only test is the main signal that test-first was skipped.

Rules: [tests](../../../engineering.md#tests), [test layout](../../../../docs/TEST-LAYOUT.md).

## What to look for

- **Same change.** Every behavior change ships its test. Every fix has a regression test that fails without it.
- **Failure paths.** Typed errors asserted, not only the happy path.
- **Determinism.** Paused Tokio time, never sleep. No real network. Environment mutation serialized; cwd changes use `CurrentDirGuard`.
- **Doubles.** Reused from `rskit-testutil`. A double proves a contract, not real adapter, process, or TLS behavior; keep integration proof.
- **Fixtures.** Large config lives in fixtures, not inline strings.
- **Coverage.** Floors in `toven.toml`; read the report rather than assuming a gap.

## Severity

| Severity | Examples |
|---|---|
| Blocker | Behavior change or fix without a test; timing-dependent flake |
| Should-fix | Missing failure-path test; real sleep; unguarded env or cwd mutation; hand-rolled fake that exists in `rskit-testutil` |
| Nit | Test name that does not describe the behavior |

## Detection starters

```bash
rg 'thread::sleep' core contrib --glob '*.rs'
rg 'set_var|remove_var|set_current_dir' core contrib --glob '*.rs'
rg 'struct \w*(Fake|Mock|Stub|Dummy)' core contrib --glob '*.rs'
make test C=<crate> T=<pattern>
make coverage-changed
```
