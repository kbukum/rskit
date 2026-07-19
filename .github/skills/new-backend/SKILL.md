---
name: new-backend
description: >-
    Add a pluggable backend/adapter (storage, cache, messaging, inference, llm, media,
    vectorstore) to rskit the canonical way — a contrib crate under contrib/<domain>/<name>
    implementing the core trait, selected via config through an explicit typed registration, no
    import-time side effects, with the in-memory/local default kept in core. Use when integrating
    a provider like S3, Kafka, Redis, Qdrant, or an LLM/inference provider.
---

# Adding a backend adapter to rskit

rskit's data/ai/infra domains use a trait + registration pattern so a core crate ships an in-memory
or local default
and heavy provider backends live in opt-in **contrib crates** behind facade feature flags.
Follow the existing owners exactly — do not invent a new registration mechanism.

## The binding rules

1. **Contrib crate.** The adapter lives at `contrib/<domain>/<name>/` (`contrib/storage/s3`, `contrib/messaging/kafka`, `contrib/cache/redis`, `contrib/vectorstore/qdrant`, `contrib/media/ffmpeg`, …),
   covered by the `contrib/Cargo.toml` member pattern. It carries the heavy SDK dependency
   so core stays light.
2. **Implements the core trait.** The adapter implements the canonical trait its core crate owns (e.g. the storage/cache/messaging provider trait)
   — it does not redefine the abstraction.
3. **Explicit typed registration, config-driven selection.** Registration is explicit
   and caller-driven with typed config captured in the factory; **no import-time side effects,
   no mutable global registry**.
   Selection is config-driven (a `lazy_static!`/`static mut`/ init-on-import registry is a blocker).
4. **Core keeps the default.** The in-memory / local backend stays in the core crate
   and remains the zero-config default; contrib backends are selected via config
   and exposed through the `rskit` facade **behind a feature flag**.

Study an existing adapter under `contrib/<domain>/` for the exact shape before writing a new one.

## Steps

1. **Create the contrib crate** (see the `new-crate` skill for workspace wiring):

   ```bash
   cargo new --lib contrib/<domain>/<name>          # e.g. contrib/storage/s3
   ```

Add it to the `contrib/Cargo.toml` member pattern
and wire a feature flag in the `rskit` facade for the integration.

2. **Define a typed `Config`** for the backend (endpoint, credentials source, timeouts, bucket/topic names).
   No stringly-typed escape hatch. Validate it at construction — this is a trust boundary.

3. **Implement the adapter** against the core trait. Timeout every remote call;
   bounded jittered retries for idempotent ops only;
   degrade/circuit-break rather than success-shaped fallbacks. Tokens go in headers,
   not query strings. No `unwrap()`/`expect()` on runtime paths;
   typed `AppError`/`AppResult` preserving cause.
   Split code by concern into focused files (config, client, adapter, mapping).

4. **Expose explicit registration** that closes over the config
   and installs the typed factory into the passed registry. No global registry,
   no import-time side effects.

5. **Crate docs** (`#![warn(missing_docs)]` + `//!`) describing the backend, its config,
   and its failure modes.

6. **Tests** — behavioral, deterministic,
   `tokio::time::pause()`/`advance()` (never `std::thread::sleep`), cover failure paths;
   fixtures over embedded config; green under race/shuffle/parallel.
   Integration tests that need a live broker/store are gated/skipped without it.

## Validate

```bash
make fmt
make build C=<crate>                       # e.g. C=rskit-storage-s3 (confirm the crate name)
make lint  C=<crate>
make test  C=<crate>
make check-topology
```

## Checklist

- [ ] Contrib crate under `contrib/<domain>/<name>/`, added to `contrib/Cargo.toml`
- [ ] Facade feature flag wired; core in-memory/local default untouched and still zero-config
- [ ] Typed `Config`, validated at construction; no `Any`/stringly-typed factory
- [ ] Explicit registration; no import-time side effects, no mutable global registry
- [ ] Timeouts, bounded retries (idempotent only), no success-shaped fallbacks, typed errors
- [ ] Crate docs + behavioral tests green under race/shuffle/parallel

Per repo workflow, **create the branch and make edits only** — the maintainer commits and pushes.
