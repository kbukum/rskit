---
name: new-backend
description: "rskit: Add an opt-in, config-selected backend implementing the owning module's typed contract."
---

# Adding a backend adapter to rskit

rskit's data/ai/infra domains use a trait + registration pattern so a core crate ships an in-memory or local default and heavy provider backends live in opt-in **contrib crates** behind facade feature flags. Follow the existing owners exactly — do not invent a new registration mechanism.

## The binding rules

1. **Contrib crate.** The adapter lives at `contrib/<domain>/<name>/` (`contrib/storage/s3`, `contrib/messaging/kafka`, `contrib/cache/redis`, `contrib/vectorstore/qdrant`, `contrib/media/ffmpeg`, …), covered by the `contrib/Cargo.toml` member pattern. It carries the heavy SDK dependency so core stays light.
2. **Implements the core trait.** The adapter implements the canonical trait its core crate owns (e.g. the storage/cache/messaging provider trait) — it does not redefine the abstraction.
3. **Explicit typed registration, config-driven selection.** Registration is explicit and caller-driven with typed config captured in the factory; **no import-time side effects, no mutable global registry**. Selection is config-driven (a `lazy_static!`/`static mut`/ init-on-import registry is a blocker).
4. **Core keeps the default.** The in-memory / local backend stays in the core crate and remains the zero-config default; contrib backends are selected via config and exposed through the `rskit` facade **behind a feature flag**.

Study an existing adapter under `contrib/<domain>/` for the exact shape before writing a new one.

## Steps

1. **Create the contrib crate** (see the `new-crate` skill for workspace wiring):

   ```bash
   cargo new --lib contrib/<domain>/<name>          # e.g. contrib/storage/s3
   ```

Add it to the `contrib/Cargo.toml` member pattern and wire a feature flag in the `rskit` facade for the integration.

2. **Define a typed `Config`** for the backend (endpoint, credentials source, timeouts, bucket/topic names). No stringly-typed escape hatch. Validate it at construction — this is a trust boundary.

3. **Implement the adapter** against the core trait, following [security and runtime](../../engineering.md#security-and-runtime): timeouts, bounded idempotent retries, header tokens, typed errors. Split code by concern (config, client, adapter, mapping).

4. **Expose explicit registration** that closes over the config and installs the typed factory into the passed registry. No global registry, no import-time side effects.

5. **Crate docs** (`//!`, with workspace lints inherited) describing the backend, its config, and its failure modes.

6. **Tests** follow [tests](../../engineering.md#tests). Integration tests that need a live broker or store are `#[ignore]`d per [test layout](../../../docs/TEST-LAYOUT.md).

## Validate

```bash
make fmt C=<crate>
make build C=<crate>                       # e.g. C=rskit-storage-s3 (confirm the crate name)
make lint  C=<crate>
make test  C=<crate>
make doc   C=<crate>
make check-topology
```

## Checklist

- [ ] Contrib crate under `contrib/<domain>/<name>/`, added to `contrib/Cargo.toml`
- [ ] Facade feature flag wired; core in-memory/local default untouched and still zero-config
- [ ] Typed `Config`, validated at construction; no `Any`/stringly-typed factory
- [ ] Explicit registration; no import-time side effects, no mutable global registry
- [ ] Timeouts, bounded retries (idempotent only), no success-shaped fallbacks, typed errors
- [ ] Crate docs, workspace lints, and deterministic behavioral tests

Leave edits uncommitted. Git actions need authorization.
