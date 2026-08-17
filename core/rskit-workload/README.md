# rskit-workload — Provider-Based Workload Orchestration

Provider-agnostic contract for deploying and managing long-running workloads (containers, pods, or any runtime unit) with an explicit backend registry and a lifecycle-managed component.

[![CI](https://github.com/kbukum/rskit/actions/workflows/ci.yml/badge.svg)](https://github.com/kbukum/rskit/actions/workflows/ci.yml) [![crates.io](https://img.shields.io/crates/v/rskit-workload.svg)](https://crates.io/crates/rskit-workload) [![docs.rs](https://docs.rs/rskit-workload/badge.svg)](https://docs.rs/rskit-workload) [![License: MIT](https://img.shields.io/badge/license-MIT-blue.svg)](https://github.com/kbukum/rskit/blob/main/LICENSE)

Mirrors gokit's `workload` package: the same concept and vocabulary, in idiomatic Rust. This crate owns the orchestration contract only — concrete backends (Docker, Kubernetes, …) live in separate adapter crates that register a `ManagerFactory`; no backend is wired in implicitly.

## Features

- `Manager` trait: deploy / stop / remove / restart / status / wait / logs / list / health check
- Optional capability traits: `ExecCapable`, `StatsCapable`, `LogStreamer`, `EventWatcher`,
  `SystemInfoCapable`, `DiskUsageCapable`, `ImageInspector`, `ImageEventWatcher`
- `WorkloadRegistry` — explicit, injected backend selection by provider name
- `WorkloadComponent` — `rskit-component` lifecycle (start / stop / health)
- `WorkloadState` / `RestartPolicy` enums and CPU/memory quantity helpers

## Usage

```toml
[dependencies]
rskit-workload = "0.2.0-alpha.4"
```

```rust
use rskit_workload::{WorkloadComponent, WorkloadConfig};
use rskit_component::Component;

# #[tokio::main]
# async fn main() -> rskit_errors::AppResult<()> {
let component = WorkloadComponent::new(WorkloadConfig::default());
component.start().await?;
assert!(component.health().is_healthy());
component.stop().await?;
# Ok(())
# }
```

Register a backend factory into a `WorkloadRegistry`, then pass it to `WorkloadComponent::with_registry` with an enabled `WorkloadConfig` to run a real provider.

## License

MIT
