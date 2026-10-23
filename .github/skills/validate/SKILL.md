---
name: validate
description: "rskit: Run the repository's build, test, lint, and documentation gates scoped to the change."
---

# Validating rskit changes with make/cargo

rskit is a multi-workspace Rust monorepo (`core/`, `contrib/`, `examples/`, `fuzz/`). The `Makefile` is the canonical task runner: it wraps `cargo` with crate/workspace scoping and the placement/supply-chain guards. Prefer it over raw `cargo` for anything with a `make` target, and **always scope to what changed** — full-tree gates are slow and belong to audits/CI sign-off.

## Golden rule: scope to what changed

Never run the whole tree for a small change. Scope by crate (`C=`), workspace (`W=core|contrib|examples`) or module group (`G=<group>`), or let the affected-set targets compute the affected area. Most targets run through Toven; `make deny` and `make check-facade-features` call `cargo` directly. `make help` lists every target.

```bash
make test-affected                        # only crates the diff touches (+ dependents), all and default features
make coverage-changed                     # coverage for changed crates
```

## Core tasks

| Intent | Command | Notes |
|---|---|---|
| Build | `make build C=<crate>` | `cargo build` |
| Test | `make test C=<crate> T=<pattern>` | nextest, all features; `PROFILE=ci` for the CI profile |
| Test (default features) | `make test-default C=<crate>` | nextest, default features only |
| Lint | `make lint C=<crate>` | clippy, all targets and features, `-D warnings` |
| Format (write) | `make fmt` | rustfmt |
| Format (check) | `make fmt-check` | fast, whole-tree |
| Doc | `make doc C=<crate>` | private items, `-D warnings` |
| Doctests | `make test-doc C=<crate>` | doctests only |
| Deny | `make deny` | topology, L7 edges, dep sync, public API, then cargo-deny (licenses, advisories, sources, bans) |
| Structure | `make structure` | declare-only aggregator guard (`lib.rs`/`mod.rs`) |

## Scoping selectors

- `C=<crate>` — one crate, e.g. `C=rskit-storage`, `C=rskit-di`.
- `W=core|contrib|examples` — one workspace.
- `G=<group>` — one module group from `toven.toml` (`toven groups` lists them).
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
make check-topology            # specific forbidden edges (layer direction is enforced by Toven groups)
make check-public-api          # only when a public surface changed
make check-workspace-deps-sync # shared dep versions consistent across workspaces
make check-facade-features     # facade feature wiring
```

`make check-<group>` lints and tests one module group, e.g. `make check-core` or `make check-transport`. Toven also rejects any dependency that reaches outside a group's allowed layers.

## Before you hand work off

For a self-contained change, the minimum passing standard is: `fmt-check`, `lint C=<crate>`, `test C=<crate>`, `doc C=<crate>` if docs changed, and `check-topology` on any structural change. Escalate to the full canonical gate only for audits or a release:

```bash
make check                     # full canonical gate — fmt-check + structure + lint + build + test + doctests + Python tooling
make deny                      # + supply-chain / topology / public-api / dep-sync
toven run readiness            # pinning, panic, SAFETY, audit, and fuzz-target sweep (CI security job)
```

Treat a green run as **necessary but not sufficient**. Rules without a gate are listed in [known gaps](../../engineering.md#known-gaps); those are on the reviewer.

Leave edits uncommitted. Git actions need authorization.
