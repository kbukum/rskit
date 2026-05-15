# rskit-observability — OpenTelemetry Integration

OpenTelemetry tracing, metrics, logs, and context propagation.

[![CI](https://github.com/kbukum/rskit/actions/workflows/ci.yml/badge.svg)](https://github.com/kbukum/rskit/actions/workflows/ci.yml)
[![crates.io](https://img.shields.io/crates/v/rskit-observability.svg)](https://crates.io/crates/rskit-observability)
[![docs.rs](https://docs.rs/rskit-observability/badge.svg)](https://docs.rs/rskit-observability)
[![License: MIT](https://img.shields.io/badge/license-MIT-blue.svg)](LICENSE)
[![MSRV: 1.85](https://img.shields.io/badge/MSRV-1.85-orange.svg)](https://blog.rust-lang.org/2025/02/20/Rust-1.85.0.html)

## Features

- `tracer_provider()` / `init_tracer()` — injectable OTLP trace provider with configurable sampling via `TracingConfig`
- `init_metrics()` — injectable OTLP metrics pipeline via `MetricsConfig`
- `init_logs()` — injectable OTLP log provider via `LogsConfig`
- `OperationContext` — tracks service, operation, request ID, and elapsed time
- `ServiceHealth` — aggregate health status across registered components
- `HealthStatus` — Healthy / Degraded / Unhealthy
- W3C Trace-Context propagation (`extract_trace_context` / `inject_trace_context`)
- OTLP over gRPC and HTTP/protobuf via `OtlpProtocol`

## Usage

```toml
[dependencies]
rskit-observability = "0.1"
```

```rust
use rskit_observability::{OperationContext, ServiceHealth, HealthStatus};

let ctx = OperationContext::new("order-svc", "create_order", "req-1", "user-42");
// ... do work ...
ctx.end_operation("success", None);
let _elapsed = ctx.elapsed();

let health = ServiceHealth::new("order-svc", "0.1.0");
health.register("db");
health.update("db", HealthStatus::Healthy, None);
assert!(health.is_healthy());
```

## See Also

[Main repository README](https://github.com/kbukum/rskit)
