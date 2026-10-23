# Lens: code quality and redesign

Catch debt that compiles cleanly but should not land. rskit is pre-1.0, so prefer a root-cause redesign over a patch.

Rules: [principles](../../../engineering.md#principles), [code style](../../../engineering.md#code-style), [naming and structure](../../../../docs/naming-and-structure.md).

## What to look for

- **Root cause.** No compatibility shims or symptom patches. Defects in the touched area are in scope. Choose Redesign, Align, Enhance, or Drop.
- **Dead code.** No callers, one-impl abstractions, commented-out code, leftover scaffolding.
- **Clarity.** The next reader understands it without the author. Names match rskit vocabulary. No hidden coupling.
- **Current idioms.** Edition 2024 and the MSRV in `clippy.toml`. `derive` over manual impls; no needless clones.
- **Locks.** `parking_lot::Mutex`; async locks only across `.await`.
- **CLI output (`rskit-cli` only).** Clear, consistent messages and exit codes; errors say what to do next.

## Severity

| Severity | Examples |
|---|---|
| Blocker | Compatibility shim or workaround that hides a broken contract |
| Should-fix | Dead code; speculative generality; outdated idiom; concern-mixed function |
| Nit | Minor naming or ordering preference |

## Detection starters

```bash
rg 'TODO|FIXME|HACK|legacy|back.?compat|for now' core/*/src contrib/*/*/src
rg '^\s*// *(let |fn |if |match |use )' core/*/src contrib/*/*/src   # commented-out code
make lint C=<crate>
```
