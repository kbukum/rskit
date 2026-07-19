# core-cli

A command-line tool built **entirely on rskit core crates** — no transport or server crate, and no `rskit` facade.

This example is a standing regression guard for the "consumer classes" goal described in [`docs/CONSUMER-CLASSES.md`](../../docs/CONSUMER-CLASSES.md): it proves the foundation layer is usable to build a real CLI for the *app / CLI / tool* consumer classes without pulling in a service stack. If a core crate ever grows a service-shaped dependency, this example's dependency tree — and the CI build — will surface it.

## What it demonstrates

- **Typed, strict config** — `rskit-config` loads a single TOML file with `deny_unknown_fields` (used with `default-features = false`, so no validator stack is linked).
- **Logging vocabulary + setup** — `rskit-logging` owns `LoggingConfig` and installs the subscriber via `init_logging`.
- **CLI output and exit codes** — `rskit-cli` renders key-value output and maps `AppError` to a conventional process exit code.
- **Typed errors** — `rskit-errors` `AppResult`/`AppError` throughout, with cause preserved.
- **Cooperative cancellation** — `rskit-cli::CancellationToken` plus `tokio::signal::ctrl_c` wind down a bounded work loop.

## Usage

```bash
cargo run -p core-cli -- version
cargo run -p core-cli -- show examples/core-cli/fixtures/app.toml
cargo run -p core-cli -- run 5
```

## Crates linked

`rskit-config`, `rskit-logging`, `rskit-cli`, `rskit-errors`, `rskit-version` — all `core/` crates. Intentionally **none** of `rskit-server`, `rskit-http`, `rskit-httpclient`, `rskit-grpc`, `rskit-sse`, or `rskit-messaging`.
