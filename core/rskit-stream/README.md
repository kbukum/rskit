# rskit-stream

Foundational async stream toolkit for rskit.

This crate owns the opinion-free,
layer-zero building blocks of the "observe → fan-out → consume" graph that recurs across config reloads,
service discovery, cache invalidation, secret rotation, and message consumers:

- **`Broadcaster<T>` / `BroadcastStream<T>`** — bounded, cancellable fan-out bus.
- **`from_slice` / `from_fn` / `from_channel`** — stream source constructors.
- **`SpawnedTask` / `TaskGroup`** — cancellable owned tasks with drain-then-abort shutdown.
- **`RskitStreamExt`** —
  `futures::Stream` extension operators (`rmap`, `rfilter`, `rfan_out`, `rparallel`, `rbatch`, windowing, throttling, and more).
- **`collect` / `drain` / `for_each`** — terminal sink combinators.

Sequential, named-step workflow orchestration is a different concern
and lives in [`rskit-chain`](../rskit-chain) (typed step sequences with progress and cancellation).

License: MIT
