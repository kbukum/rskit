# Consumer Classes

rskit is a general-purpose foundation, not a service framework.
Its core crates are designed to serve several kinds of consumer without forcing a long-running network service on any of them.

This note defines those consumer classes
and the guarantees the foundation layer (`core/rskit-*`) makes to each,
so new crates are designed consumer-neutral from day one.

## The classes

| Class | What it is | Typical entry points |
|---|---|---|
| **Service** | A long-running networked process (HTTP/gRPC server, message consumer) with readiness, health, and graceful shutdown. | `App::run`, `rskit-server`, `rskit-grpc`, `rskit-messaging` |
| **App** | A long-running process that is *not* primarily a network server (daemon, worker, scheduler). | `App::run_task`, `rskit-worker`, `rskit-dag` |
| **CLI** | A short-lived terminal command driven by argv and exit codes. | `rskit-cli`, `rskit-config` strict/typed load |
| **Tool** | A non-interactive batch/automation step (CI task, codegen, migration). | `rskit-config` `toml()`/strict load, `rskit-process` |
| **Library** | A crate that builds on rskit but exposes its own API and runs inside someone else's process. | typed `load`, `rskit-errors`, plain core crates with `default-features = false` |

## What the foundation guarantees every class

These hold for every `core/rskit-*` crate, regardless of consumer class:

- **No mandatory service runtime.** Using a core crate never forces a network listener,
  an always-on telemetry exporter, or a background server. Heavy,
  service-shaped concerns (tracing subscriber, OTLP export, network clients) are feature-gated
  or injected.
- **No import-time side effects.** No global mutable registries and no init-on-import;
  everything is injected or config-driven.
- **Typed, minimal APIs.** Public surfaces avoid `Any`-style escape hatches;
  errors are `AppError` with `ErrorCode` and a preserved cause;
  growable enums are `#[non_exhaustive]`.
- **Downward-only layering.** Core crates depend only on lower layers;
  they never reach up into transport, composition, or service-shaped crates.
- **No runtime panics on the happy path.** No `unwrap`/`expect`/swallowed errors outside tests,
  and no success-shaped fallbacks.
- **Lifecycle ownership where work is spawned.** Anything that spawns work exposes ownership,
  cancellation, timeout, and shutdown.

## What each class additionally gets

- **Service** — readiness/health surfaces and graceful drain via the transport crates
  and `App` lifecycle; opt-in observability and auth layers.
- **App** — the same lifecycle without a listener: `App` supports a one-shot/task path,
  and `rskit-worker`/`rskit-dag` provide bounded, cancellable concurrency.
- **CLI** — `rskit-cli` for progress, structured output, exit-code conventions,
  and Ctrl+C cancellation;
  `rskit-config` for deterministic single-file typed/strict loading with no implicit environment
  or dotenv reads.
- **Tool** — deterministic configuration (`toml()`/strict loaders, no ambient env),
  argv-only subprocess execution via `rskit-process`,
  and typed errors suitable for non-interactive exit handling.
- **Library** — the leanest footprint:
  depend on individual core crates with `default-features = false` to drop optional stacks (for example, `rskit-config` without the `validate` feature, or `rskit-logging` as the configuration vocabulary only).

## Regression guard

The [`core-cli`](../examples/core-cli) example is a standing guard against service-shaped creep.
It builds a real CLI on `rskit-config`, `rskit-logging`, `rskit-cli`, `rskit-errors`,
and `rskit-version`, and intentionally links **none** of `rskit-server`, `rskit-http`,
`rskit-httpclient`, `rskit-grpc`, `rskit-sse`, or `rskit-messaging`.

If a core crate ever grows a transport/server dependency, that example's dependency tree —
and the CI build — changes,
which is the signal to re-evaluate the crate against the guarantees above.
