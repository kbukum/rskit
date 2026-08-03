# 0001. Layered crate architecture

- Status: Accepted
- Date: 2026-04-26
- Authors: @kbukum

## Context

rskit is a split Cargo workspace family with 70+ publishable crates across `core/` and `contrib/`. Without an enforced layering rule, foundation crates (e.g. `rskit-errors`) could accidentally depend on higher layers (e.g. `rskit-server`), creating cycles, slow rebuilds, and undermining the modular distribution model. The sibling repos ([`gokit`](https://github.com/kbukum/gokit), [`pykit`](https://github.com/kbukum/pykit)) enforce the same multi-tier layering model with language-native linters (`depguard`/`import-linter`).

We need a stable rule that engineers can apply without case-by-case debate, enforced automatically.

## Decision

The domain graph organizes rskit crates into the following layers (lowest depends on nothing higher):

1. **Core** — `rskit-errors`, `rskit-config`, `rskit-logging`, `rskit-validation`, `rskit-encryption`, `rskit-util`, `rskit-fs`, `rskit-version`, `rskit-schema`, `rskit-codec`, `rskit-stream`
2. **Patterns** — `rskit-component`, `rskit-hook`, `rskit-provider`, `rskit-di`
3. **Cross-cutting** — `rskit-observability`, `rskit-resilience`, `rskit-security`
4. **Composition** — `rskit-bootstrap`, `rskit-dag`, `rskit-chain`, `rskit-worker`, `rskit-process`, `rskit-stateful`
5. **Transport** — `rskit-server`, `rskit-httpclient`, `rskit-grpc`, `rskit-sse`, `rskit-http`, `rskit-discovery`
6. **Auth** — `rskit-auth`, `rskit-authz`
7. **Data** — `rskit-database`, `rskit-database-sqlite`, `rskit-cache`, `rskit-cache-redis`, `rskit-storage`, `rskit-storage-gcs`, `rskit-storage-s3`, `rskit-storage-supabase`, `rskit-vectorstore`, `rskit-vectorstore-qdrant`, `rskit-messaging`, `rskit-messaging-kafka`, `rskit-messaging-nats`, `rskit-messaging-rabbitmq`
8. **AI** — `rskit-ai`, `rskit-llm`, `rskit-llm-*`, `rskit-embedding`, `rskit-inference`, `rskit-inference-*`, `rskit-agent`, `rskit-tool`, `rskit-skill`, `rskit-mcp`
9. **Media** — `rskit-media`, `rskit-media-audio`, `rskit-media-ffmpeg`, `rskit-media-image`
10. **Infra** — `rskit-bench`, `rskit-cli`, `rskit-dataset`, `rskit-git`, `rskit-testutil`, `rskit-workload`, `rskit-suite`

When a foundation crate needs a service that lives in a higher layer (e.g. an HTTP transport for an SSE broadcaster), the foundation declares a **trait** for the operation it needs. Higher-layer types satisfy it explicitly via `impl Trait for Type`.

The layer contract is enforced via:

- [`cargo-deny`](https://embarkstudios.github.io/cargo-deny/) `bans` rules in [`deny.toml`](../../deny.toml) and the split workspace deny configs — denies forbidden dependency edges and dependency policy violations.
- The repository tooling app commands that walk the crate graph and assert topology invariants, especially `scripts/rskit_tool.py check topology` and `scripts/rskit_tool.py check l7-edges`.

## Consequences

- New crates must be placed in the correct layer; the lint rule fails CI if violated.
- Domain-free primitives shared across layers belong in `rskit-util`, which remains the L0 utility crate and must not depend on internal `rskit-*` crates.
- Cross-layer wiring lives in `rskit-bootstrap` and `rskit-di`; foundation crates remain independently testable and reusable.
- A small upfront cost: foundation crates duplicate single-method traits (e.g. `worker::Broadcaster`) instead of importing concrete transport types. This is a deliberate tradeoff for layering isolation.
- Sibling parity: the same layering decision exists in [`gokit/docs/adr/0001`](https://github.com/kbukum/gokit/blob/main/docs/adr/0001-three-tier-layering.md) and [`pykit/docs/adr/0001`](https://github.com/kbukum/pykit/blob/main/docs/adr/0001-layered-package-architecture.md); changes here should be evaluated for both.

## Alternatives considered

- **No layering** — convention alone does not hold at this crate count.
- **Three tiers (foundation / patterns / everything else)** — too coarse for 70+ publishable crates; "everything else" would still need internal ordering.
- **Per-crate allowlists** — too brittle; every new crate would require bilateral edits.
- **Move enforcement to runtime** — Rust's compile-time orientation makes static enforcement strictly preferable.
