# rskit-discovery — Service Discovery

Service discovery with registry and load balancing strategies.

[![CI](https://github.com/kbukum/rskit/actions/workflows/ci.yml/badge.svg)](https://github.com/kbukum/rskit/actions/workflows/ci.yml) [![crates.io](https://img.shields.io/crates/v/rskit-discovery.svg)](https://crates.io/crates/rskit-discovery) [![docs.rs](https://docs.rs/rskit-discovery/badge.svg)](https://docs.rs/rskit-discovery) [![License: MIT](https://img.shields.io/badge/license-MIT-blue.svg)](LICENSE) [![MSRV: 1.85](https://img.shields.io/badge/MSRV-1.85-orange.svg)](https://blog.rust-lang.org/2025/02/20/Rust-1.85.0.html)

## Features

- `Discovery` trait — resolve service instances by name
- `Registry` trait — register / deregister instances
- `ServiceInstance` with id, address, port, health, tags, and metadata
- Load balancers: `RoundRobin`, `Random`, `LeastConnections`
- `InMemoryDiscovery` for testing
- Optional Consul backend (feature-gated) using the shared hardened HTTP client

## Usage

```toml
[dependencies]
rskit-discovery = "0.1.0-alpha.1"
```

```rust
use rskit_discovery::{
    InMemoryDiscovery, ServiceInstance, Discovery, Registry,
    LoadBalancer, RoundRobin,
};
use std::collections::HashMap;

async fn example() {
    let disco = InMemoryDiscovery::new();

    let inst = ServiceInstance {
        id: "api-1".into(), name: "api".into(),
        address: "10.0.0.1".into(), port: 8080,
        healthy: true, weight: 1, tags: vec![], metadata: HashMap::new(),
    };
    disco.register(&inst).await.unwrap();

    let instances = disco.resolve("api").await.unwrap();
    let lb = RoundRobin::new();
    let pick = lb.pick(&instances);
    println!("routed to {:?}", pick.map(|i| i.endpoint()));
}
```

## Consul transport behavior

The optional Consul backend builds on `rskit-httpclient`. Consul requests use a bounded timeout that exceeds the blocking-query wait interval, do not follow redirects, and allow-list only the configured Consul host.

## See Also

[Main repository README](https://github.com/kbukum/rskit)
