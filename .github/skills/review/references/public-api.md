# Lens: public API and stability

rskit is a library. Every `pub` item is a promise to the facade, adapters, and downstream users.

Rules: [public API and stability](../../../engineering.md#public-api-and-stability), [SemVer policy](../../../../docs/policy/SEMVER.md), [deprecation policy](../../../../docs/policy/DEPRECATION.md).

## What to look for

- **Surface size.** Narrowest visibility that works: private, then `pub(crate)`, then `pub`. Re-export the call-site API from the crate root. Struct fields private unless the type is plain data.
- **Types.** No `Box<dyn Any>` or stringly typed escape hatches. Borrow inputs when ownership is not needed.
- **Errors.** `AppResult<T>` with an `ErrorCode` and cause. The message tells the caller what failed and what to do.
- **Growth.** `#[non_exhaustive]` on enums and structs that may grow. Builders or parameter structs for related or same-typed inputs. `#[must_use]` on `with_*` builders.
- **Versioning.** Breaking changes listed under `Changed` with a **Breaking** prefix. Callers updated in the same change; no compatibility shims.
- **Consumer experience.** A first-time user can adopt the crate alone with good defaults and a clear error when misconfigured.
- **Parity.** Shared wire contracts stay aligned with gokit; internal shapes follow Rust idioms.

## Severity

| Severity | Examples |
|---|---|
| Blocker | Unrecorded breaking change; public type leaking an implementation dependency; error that drops its cause |
| Should-fix | Incidental `pub`; growing enum without `#[non_exhaustive]`; long same-typed positional arguments; transliterated Go shape |
| Nit | Builder method name that could read better at the call site |

## Detection starters

```bash
rg 'pub [a-z_]+:' core/*/src contrib/*/*/src         # public fields to justify
rg 'pub enum' core/*/src contrib/*/*/src             # check #[non_exhaustive]
rg -B1 'pub fn with_' core/*/src contrib/*/*/src     # check #[must_use]
rg 'dyn Any' core/*/src contrib/*/*/src
make check-public-api                                # facade API diff (report only)
```
