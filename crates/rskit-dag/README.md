# rskit-dag — DAG Task Orchestrator

Directed acyclic graph task orchestrator with parallel execution.

[![CI](https://github.com/kbukum/rskit/actions/workflows/ci.yml/badge.svg)](https://github.com/kbukum/rskit/actions/workflows/ci.yml)
[![crates.io](https://img.shields.io/crates/v/rskit-dag.svg)](https://crates.io/crates/rskit-dag)
[![docs.rs](https://docs.rs/rskit-dag/badge.svg)](https://docs.rs/rskit-dag)
[![License: MIT](https://img.shields.io/badge/license-MIT-blue.svg)](LICENSE)
[![MSRV: 1.85](https://img.shields.io/badge/MSRV-1.85-orange.svg)](https://blog.rust-lang.org/2025/02/20/Rust-1.85.0.html)

## Features

- `Dag` — build a graph with `add_node` and `add_edge`, then `execute`
- `DagNode` trait — implement `id()` and async `execute(inputs, cancel)`
- Topological sort via Kahn's algorithm with cycle detection
- Maximum parallelism — independent nodes run concurrently
- Dependency outputs passed as `HashMap<String, serde_json::Value>`
- Cancellation via `tokio_util::sync::CancellationToken`

## Execution semantics

### FailurePolicy modes

| Mode | Behavior |
|------|----------|
| `FailurePolicy::FailFast` (default) | Return the first node error immediately. Already-running sibling tasks are allowed to finish or observe the caller's cancellation token. |
| `FailurePolicy::Continue` | Record the failed node, treat it as completed for scheduling, and keep running downstream and independent nodes. Downstream nodes receive only successful dependency outputs. |
| `FailurePolicy::SkipDependents` | Record the failed node and skip all transitive dependents while independent branches continue to run. |

### Cycle-detection guarantee

`Dag::execute` always runs a topological-sort validation before scheduling work. Cyclic graphs
return `ErrorCode::Validation` and no node is started.

### Parallel sibling execution

Nodes whose dependencies are satisfied are spawned as siblings and run concurrently. By default,
parallelism is bounded only by the Tokio runtime. Use `Dag::with_max_parallelism(n)` to cap the
number of concurrently executing nodes; `n` is clamped to at least one.

## Usage

```toml
[dependencies]
rskit-dag = "0.1"
```

```rust
use rskit_dag::{Dag, DagNode};
use rskit_errors::AppResult;
use serde_json::json;
use std::collections::HashMap;
use tokio_util::sync::CancellationToken;

struct Sum;
impl DagNode for Sum {
    fn id(&self) -> &str { "sum" }
    fn execute(&self, _inputs: HashMap<String, serde_json::Value>, _cancel: CancellationToken)
        -> std::pin::Pin<Box<dyn std::future::Future<Output = AppResult<serde_json::Value>> + Send + '_>> {
        Box::pin(async { Ok(json!({"total": 42})) })
    }
}

// let results = Dag::new().add_node(Sum).execute(CancellationToken::new()).await?;
```

## See Also

[Main repository README](https://github.com/kbukum/rskit)
