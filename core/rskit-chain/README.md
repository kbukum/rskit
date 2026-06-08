# rskit-chain

Typed sequential chain execution for rskit.

`rskit-chain` builds statically typed async chains where each step receives the previous step's output and returns the next step's input. Execution short-circuits on the first `AppError`, checks cancellation between steps, and runs cleanup handlers for completed steps when later work fails or is cancelled.

## Features

- Fluent `ChainBuilder` API for heterogeneous step composition.
- `Step::from_fn` adapters for async functions.
- Progress callbacks with step status events.
- Cleanup hooks for compensating completed steps.
- `CancellationToken` support for cooperative cancellation.
