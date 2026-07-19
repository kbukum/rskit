# Pass 02 — Principle conformance

Each item here is a hard principle from [`.github/copilot-instructions.md`](../../../copilot-instructions.md),
not a preference. This is where fast AI-assisted coding drifts most — especially around resilience,
concurrency, and composition.

> **Run in a separate, clean-context agent** — never inline in the session that wrote the code.
> An independent reviewer re-derives every judgment from the code
> and the principles instead of trusting prior reasoning.
> A plan/spec may be passed in as a scope checklist only; it never excuses a baseline violation.

**Scope note.** *Changes mode:* grep the touched crates and reason about each runtime path.
*Project mode:* the panic/concurrency/composition invariants below hold across the whole library surface
— sweep all of `core/` and `contrib/`.

## Typed, minimal APIs

No broad `Any` / `Box<dyn Any>` / unchecked escape hatches in public surfaces.
Actionable typed errors that preserve cause. Minimal public surface —
no incidental `pub` (the public-API guardrail `make check-public-api` backs this; an unintended surface change shows up there).

- **Narrowest visibility that works.** Internal helpers stay private;
  items shared only within the crate are `pub(crate)`,
  items shared only with the parent module are `pub(super)`.
  Reserve `pub` for the intended external API
  and re-export it flatly from the crate root (`pub use`) rather than exposing deep module paths.
  `unreachable_pub` (enabled in every workspace's `[lints]`) flags `pub` that can't be reached externally
  — treat a hit as "should be `pub(crate)`".
- **Struct fields are private by default.** A `pub` struct does not imply `pub` fields —
  keep fields private
  and expose a constructor (`new` / `with_*` builder) plus getters where consumers need read access.
  Public fields are justified only for plain data (`#[non_exhaustive]`) holders with no invariant to uphold;
  an incidental `pub` field that leaks a representation detail is a should-fix.

## Errors & resilience

- No panics / `unwrap()` / `expect()` / swallowed errors on runtime paths (tests excepted).
- No success-shaped fallbacks that mask failure.
- Every remote call has a **timeout**. Retries are **bounded, jittered,
  and applied to idempotent ops only**. Failures circuit-break
  and degrade gracefully rather than hang or cascade. (Reuse `rskit-resilience` — see pass `01`.)

## Concurrency

- Every spawned task has clear **ownership, cancellation, timeout, and shutdown** handling.
- Queues / buffers / concurrency are **bounded with documented backpressure**;
  components **drain on shutdown**.
- An unbounded channel, or a task with no cancellation path, is a **blocker**.

## Composition

- Registries and policies are **explicitly injected**; selection is config-driven.
- **No import-time side effects, no mutable global registries**,
  no reaching for a global logger/tracer — inject them.
- A `lazy_static!` / `static mut` registry or init-on-import is a **blocker**.

## Keep code current

Current idioms and standards, not old habits (also enforced in pass `01`).
Flag patterns superseded by edition 2024 / msrv 1.91 idioms.

## AI / model features (only if the change touches them)

Model output and retrieved context are **untrusted**; outputs are structured/validated;
tool calls are least-privilege with a **human gate on destructive actions**;
prompts/models are versioned and changes gated on evals.

## Detection starters

Exclude `#[cfg(test)]` and `tests/` when judging runtime-path hits.

```bash
rg '\.unwrap\(\)|\.expect\(' core/*/src contrib/*/*/src
rg 'dyn Any|Box<dyn Any>' core/ contrib/
rg 'lazy_static|static mut|once_cell::sync::Lazy' core/ contrib/    # global-registry / import-time smell
rg 'unbounded_channel|channel\(\)|spawn\(' core/ contrib/          # check for bounded + cancellation
rg 'tokio::spawn' core/ contrib/                                    # each needs ownership/cancellation/shutdown
rg 'pub [a-z_]+:' core/*/src contrib/*/*/src                       # pub struct fields — justify or make private
```

Then `make check-public-api` for the minimal-surface guardrail.
