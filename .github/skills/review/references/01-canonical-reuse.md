# Pass 01 — Canonical-owner reuse

rskit *is* the canonical toolkit, so the duplication risk is internal: **did the change reimplement something an existing core crate (or the standard library) already owns?** Fast AI-assisted code often reaches for a fresh local helper instead of the owner — assume duplication until proven otherwise. Treat findings here as a blocker class.

> **Run in a separate, clean-context agent** — never inline in the session that wrote the code. An independent reviewer re-derives every judgment from the code and the principles instead of trusting prior reasoning. A plan/spec may be passed in as a scope checklist only; it never excuses a baseline violation.

**Scope note.** *Changes mode:* for each new type/helper in the diff, name the concern and find its owner. *Project mode:* sweep `core/` and `contrib/` for the patterns below and reconcile each against the owning crate — long-lived internal forks are exactly what this pass exists to surface.

## The rule

Reuse or enhance the canonical owner before writing new code. Never duplicate a shared concern — **errors, config, logging, auth, retries/resilience, observability, HTTP, registries, validation, process, di**. If the owner is inadequate, enhance it *generically* rather than forking a copy in another crate. rskit must stay foundational and multi-purpose: a fix belongs in the owner so every consumer benefits.

## How to check thoroughly

The canonical owner set is documented in [`docs/CONCERN-OWNERS.md`](../../../../docs/CONCERN-OWNERS.md) — start there, then reconcile each low-level operation against it. For each candidate, name the concern, locate its owning core crate, and confirm the change calls the owner rather than rewriting it:

- **Errors.** rskit `AppError` / `AppResult` with `ErrorCode`, cause preserved. A new error enum, a `thiserror` type, or a `String` error for a shared concern is duplication.
- **Resilience.** Retries / timeouts / circuit-breaking come from `rskit-resilience`, not hand-rolled loops or ad-hoc `tokio::time::timeout` scattering.
- **Config / logging / di / observability.** Route through the owning core crate; no parallel re-implementation, no second logger/tracer setup.
- **HTTP / transport.** Reuse `rskit-http` / `rskit-httpclient`; a raw `reqwest` / `hyper::Client` / `TcpStream` in an adapter is duplication.
- **Concurrency primitives.** `parking_lot::Mutex`, never `std::sync::Mutex`. Worker/queue patterns come from `rskit-worker`, not custom task loops.
- **Keep code current (part of reuse).** Before adding a dependency or helper, verify the standard library does not already cover it, the dependency is maintained, and no open CVE applies. Reinventing a std facility is a should-fix.
- **"Almost the same" counts.** A near-copy with one tweaked line is still a fork — enhance the owner to cover the new case.

## Detection starters

These flag candidates, not verdicts — read each hit, then name the owner that should have been used.

```bash
rg 'thiserror|#\[derive\(.*Error|impl .*Error for' core/ contrib/
rg 'std::sync::Mutex' core/ contrib/                 # should be parking_lot::Mutex
rg 'std::process::Command|Command::new' core/ contrib/
rg 'reqwest|hyper::Client|TcpStream' contrib/        # HTTP/transport should reuse rskit-http
rg 'tokio::time::timeout|retry|backoff' core/ contrib/   # resilience should come from rskit-resilience
```

For each hit: is there a core-crate owner for this concern? If yes and the code does not use it → **blocker** (reuse). If no owner exists and it is a genuinely foundational concern → it should be **added to the owning crate** (or a new core crate), not solved locally; a local solution is a **should-fix** with an "upstream to the owner" note.

## Output for this pass

Per finding, name the concrete core crate/type that should have been used (e.g. "use `rskit-resilience` retry policy instead of a hand-rolled loop", "wrap with `rskit-process` rather than `std::process::Command`").
