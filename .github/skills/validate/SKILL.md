---
name: validate
description: >-
    Build, test, lint, format-check, doc, and vuln/license-scan rskit changes through make and
    cargo — scoped to the crates that actually changed. Use whenever you need to validate an
    rskit change, run tests for a crate, reproduce CI locally, or check the blast radius of an
    edit before committing.
---

# Validating rskit changes with make/cargo

rskit is a multi-workspace Rust monorepo (`core/`, `contrib/`, `examples/`) with 50+ crates. The
`Makefile` is the canonical task runner: it wraps `cargo` with crate/workspace scoping and the
placement/supply-chain guards. Prefer it over raw `cargo` for anything with a `make` target, and
**always scope to what changed** — full-tree gates are slow and belong to audits/CI sign-off.

## Golden rule: scope to what changed

Never run the whole tree for a small change. Scope by crate (`C=`), by workspace
(`W=core|contrib|examples`), or let the affected-set targets compute the blast radius.

```bash
make test-affected                        # only crates the diff touches (+ reverse-deps)
make coverage-changed                     # coverage for changed crates
```

## Core tasks

| Intent | Command | Notes |
|---|---|---|
| Build | `make build C=<crate>` | `cargo build`; `W=` for a whole workspace |
| Test | `make test C=<crate> T=<pattern>` | defaults to `--all-features` |
| Lint | `make lint C=<crate>` | clippy `-D warnings`; defaults to `--all-features` |
| Format (write) | `make fmt` | rustfmt |
| Format (check) | `make fmt-check` | fast, whole-tree |
| Doc | `make doc C=<crate>` | `-D warnings` |
| Doctests | `make test-doc C=<crate>` | doctests only |
| Deny | `make deny` | cargo-deny: licenses, advisories, sources |
| Structure | `make structure` | declare-only aggregator guard (`lib.rs`/`mod.rs`) |

## Scoping selectors

- `C=<crate>` — one crate, e.g. `C=rskit-storage`, `C=rskit-di`.
- `W=core|contrib|examples` — one workspace.
- `T=<pattern>` — a test-name filter passed through to the test runner.

```bash
make test C=rskit-di T=cycle                # one crate, tests matching "cycle"
make lint W=core                            # clippy across the core workspace
make build C=rskit-server                   # one crate
```

To stay scoped below what a `make` target offers, drive `cargo` directly:

```bash
cargo test -p rskit-di --all-features -- --include-ignored
cargo clippy -p rskit-di --all-targets --all-features -- -D warnings
```

## Placement, API, and topology guards

These are cheap and catch what the compiler won't:

```bash
make check-topology            # crate placement / acyclic layering (run on any structural change)
make check-public-api          # only when a public surface changed
make check-workspace-deps-sync # shared dep versions consistent across workspaces
make check-facade-features     # facade feature wiring
```

Per-domain gates aggregate the above for a slice of the tree:
`make check-core|check-data|check-transport|check-auth|check-ai|check-media|check-infra|check-crosscutting|check-composition|check-patterns`.

## Before you hand work off

For a self-contained change, the minimum green bar is: `fmt-check`, `lint C=<crate>`,
`test C=<crate>` (green under race/shuffle/parallel), `doc C=<crate>` if docs changed, and
`check-topology` on any structural change. Escalate to the full canonical gate only for audits or
a release:

```bash
make check                     # full canonical gate — fmt-check + lint + build + test
make deny                      # + supply-chain / topology / public-api / dep-sync
make release-readiness         # supply-chain + API sweep before a release
```

Treat a green run as **necessary but not sufficient**: it does not catch unbounded concurrency,
missing timeouts/cancellation, global-registry composition smells, duplicated owners, or
boundary-validation gaps. Those are on the reviewer.

Per repo workflow, **create the branch and make edits only** — the maintainer commits and pushes.
