# Concern owners

The canonical **concern → owning crate** map for rskit. Before adding any shared helper,
type, or capability, find the concern below and **reuse or extend the named owner** — do not
fork a local copy. If the owner is inadequate, enhance it *generically* (so every consumer
benefits), never caller-specifically. Reimplementing a concern that already has an owner is a
review blocker.

This map names *who* owns each concern; the *how to judge* procedure (reuse / enhance / add /
justify) lives in the review pass
[`.github/skills/review/references/01-canonical-reuse.md`](../.github/skills/review/references/01-canonical-reuse.md).
Start here, then reconcile each low-level operation against that pass.

| Concern | Owner | Reuse this, not | Notes |
|---|---|---|---|
| Data formats (JSON/TOML/…) | `rskit-codec` | hand-rolled `serde_json` / `toml` wrappers, per-crate ser/de helpers | |
| Generic helpers (slices/maps/clock/copy/ensure-dir) | `rskit-util` + std | a fresh local helper | scoped foundation owner, not a dumping ground |
| Filesystem / path safety / atomic writes | `rskit-fs` | raw `std::fs` + manual canonicalize/escape checks | path confinement, atomic writes |
| Config loading / precedence | `rskit-config` | custom env/flag/file precedence logic | |
| Schema validation | `rskit-schema` | hand-rolled validation walks | |
| Errors | `rskit-errors` | ad-hoc error enums for shared concerns, `Box<dyn Error>` in public APIs | typed error enums, cause preserved |
| Logging / tracing | `rskit-logging` / `rskit-observability` | `println!`, direct exporter wiring | `tracing`, injected subscriber/meter |
| Resilience (retry/timeout/circuit-break) | `rskit-resilience` | hand-rolled loops, scattered timeouts + custom backoff | idempotent ops only, bounded + jittered |
| HTTP client / server | `rskit-httpclient` / `rskit-server` / `rskit-http` | raw client with custom retry/timeout | |
| Subprocess | `rskit-process` | bare `std::process::Command` | argv-only, no shell |
| Dependency injection | `rskit-di` | service-locator / string-keyed resolution | typed resolution |
| Encryption / security | `rskit-encryption` / `rskit-security` | ad-hoc crypto, custom header sets | current algorithms only |
| Git operations | `rskit-git` | bare `Command::new("git")` | |
| Validation | `rskit-validation` | inline boundary checks duplicated per crate | |

## How to use this map

1. Name the concern before writing the code.
2. Find its owner above; **consume it or implement its trait** (see the
   [reuse review pass](../.github/skills/review/references/01-canonical-reuse.md)).
3. If the owner is close but inadequate, enhance it generically, then consume — never fork.
4. If a concern has genuinely no owner and is foundational, add it to the correct owning
   crate (or a new correctly-layered one), with tests and docs — not locally.

The list is illustrative, not exhaustive: **any** rskit crate is a potential owner, so a
capability that maps to an owner not named here still counts.
