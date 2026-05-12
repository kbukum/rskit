# 0001. Layered crate architecture

- Status: Accepted
- Date: 2026-04-26
- Authors: @kbukum

## Context

rskit is a Cargo workspace with 40+ crates. Without an enforced layering
rule, foundation crates (e.g. `rskit-errors`) could accidentally depend on
higher layers (e.g. `rskit-server`), creating cycles, slow rebuilds, and
undermining the modular distribution model. The sibling repos
([`gokit`](https://github.com/kbukum/gokit),
[`pykit`](https://github.com/kbukum/pykit)) faced the same problem and
adopted multi-tier layering enforced by linters
(`depguard`/`import-linter`).

We need a stable rule that engineers can apply without case-by-case
debate, enforced automatically.

## Decision

We will organize rskit crates into the following layers (lowest depends
on nothing higher):

1. **Foundation** — `rskit-errors`, `rskit-config`, `rskit-observability`,
   `rskit-validation`
2. **Utilities** — `rskit-encryption`, `rskit-schema`, `rskit-storage`,
   `rskit-media`, `rskit-media-image`, `rskit-media-audio`,
   `rskit-media-ffmpeg`
3. **Patterns** — `rskit-provider`, `rskit-resilience`, `rskit-hook`,
   `rskit-chain`
4. **Frameworks** — `rskit-di`, `rskit-bootstrap`, `rskit-observability`
5. **Data & Flow** — `rskit-pipeline`, `rskit-dag`, `rskit-worker`,
   `rskit-sse`, `rskit-cache`
6. **Security** — `rskit-auth`, `rskit-authz`
7. **Transport** — `rskit-http`, `rskit-httpclient`, `rskit-grpc`,
   `rskit-server`
8. **Infrastructure** — `rskit-database`, `rskit-storage-s3`,
   `rskit-storage-gcs`, `rskit-messaging`
9. **AI/ML** — `rskit-llm`, `rskit-llm-providers`, `rskit-bench`,
   `rskit-dataset`, `rskit-embedding`, `rskit-inference`,
   `rskit-vectorstore`, `rskit-mcp`, `rskit-agent`, `rskit-tool`
10. **Platform** — `rskit-discovery`, `rskit-process`, `rskit-cli`,
    `rskit-explain`, `rskit-integration`, `rskit-testutil`

When a foundation crate needs a service that lives in a higher layer
(e.g. an HTTP transport for an SSE broadcaster), the foundation declares
a **trait** for the operation it needs. Higher-layer types satisfy it
explicitly via `impl Trait for Type`.

The layer contract is enforced via:

- [`cargo-deny`](https://embarkstudios.github.io/cargo-deny/) `bans` rules
  in [`deny.toml`](../../deny.toml) — denies upward dependency edges.
- A custom CI script that walks the crate graph and asserts the layer
  invariant (`scripts/check-layers.sh`).

## Consequences

- New crates must be placed in the correct layer; the lint rule fails CI
  if violated.
- Cross-layer wiring lives in `rskit-bootstrap` and `rskit-di`; foundation
  crates remain independently testable and reusable.
- A small upfront cost: foundation crates duplicate single-method traits
  (e.g. `worker::Broadcaster`) instead of importing concrete transport
  types. This is a deliberate tradeoff for layering isolation.
- Sibling parity: the same layering decision exists in
  [`gokit/docs/adr/0001`](https://github.com/kbukum/gokit/blob/main/docs/adr/0001-three-tier-layering.md)
  and [`pykit/docs/adr/0001`](https://github.com/kbukum/pykit/blob/main/docs/adr/0001-layered-package-architecture.md);
  changes here should be evaluated for both.

## Alternatives considered

- **No layering (status quo)** — relied on convention; review showed it
  doesn't hold at this crate count.
- **Three tiers (foundation / patterns / everything else)** — too coarse
  for 40+ crates; "everything else" would still need internal ordering.
- **Per-crate allowlists** — too brittle; every new crate would require
  bilateral edits.
- **Move enforcement to runtime** — Rust's compile-time orientation makes
  static enforcement strictly preferable.
