# Pass 05 — TDD and tests

rskit's standard: behavioral, deterministic, failure paths covered, **green under race / shuffle / parallel**, a regression test for every fix, tests in the **same** change. This pass catches the classic late-test signal — tests written after the fact that only assert the happy path the author already saw working.

> **Run in a separate, clean-context agent** — never inline in the session that wrote the code. An independent reviewer re-derives every judgment from the code and the principles instead of trusting prior reasoning. A plan/spec may be passed in as a scope checklist only; it never excuses a baseline violation.

**Scope note.** *Changes mode:* every behavioral change in the diff must ship its test in the same diff. *Project mode:* assess coverage of each crate's public behavior and failure paths, audit for inline config and timing flakiness, and confirm the coverage gate holds (`make release-coverage`).

## Checks

- **Test in the same change.** Every behavioral change has a corresponding test in the same diff. A feature/fix with no test is a blocker — the core TDD failure to catch.
- **Regression test per fix.** Every bug fix has a regression test that fails without the fix and passes with it.
- **Failure paths tested.** Not just the happy path; typed errors asserted, never a panic path in production code standing in for error handling.
- **Deterministic and concurrency-safe.** Green under race / shuffle / parallel. Flaky-by-timing is a blocker.
- **Time-dependent tests use `tokio::time::pause()` / `advance()`** — never `std::thread::sleep`. A real sleep in a test is a should-fix.
- **Env-var tests hold the `parking_lot::Mutex<()>` guard** to serialize — an env test without the guard is a flake source (should-fix).
- **Fixtures over embedded config.** No large inline config strings — move them to fixtures.
- **No real network access.**

## The late-test signal

If tests were clearly written *after* the implementation and only assert the happy path the author already saw working — no failure-path assertions, no regression case, no concurrency coverage on a concurrent path — call it out. That is the signal that TDD was not followed, even when coverage numbers look fine.

## Detection starters

```bash
# real sleeps and unguarded env-var mutation in tests
rg 'std::thread::sleep|thread::sleep' core/ contrib/
rg 'set_var|remove_var|env::set_var' core/*/tests core/*/src contrib/*/*/tests
# custom mocks / inline config that should be fixtures
rg 'struct .*(Fake|Mock|Stub|Dummy)' core/ contrib/
rg 'r#"|toml::from_str|json!\(' core/*/tests contrib/*/*/tests
```

Then run the focused crate tests (`make test C=<crate>`) and confirm determinism with the affected/coverage targets (`make test-affected`, `make coverage-changed`, `make release-coverage` for the gate).
