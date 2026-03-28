use std::collections::HashMap;

use rskit_discovery::{
    Discovery, InMemoryDiscovery, LoadBalancer, Random, RoundRobin, ServiceInstance,
};

fn instance(id: &str, name: &str) -> ServiceInstance {
    ServiceInstance {
        id: id.into(),
        name: name.into(),
        address: "127.0.0.1".into(),
        port: 8080,
        healthy: true,
        tags: vec![],
        metadata: HashMap::new(),
    }
}

// ── InMemoryDiscovery ───────────────────────────────────────────────

#[tokio::test]
async fn register_resolve_deregister() {
    let disco = InMemoryDiscovery::new();

    disco.register(&instance("a", "svc")).await.unwrap();
    disco.register(&instance("b", "svc")).await.unwrap();

    let resolved = disco.resolve("svc").await.unwrap();
    assert_eq!(resolved.len(), 2);

    disco.deregister("a").await.unwrap();
    let resolved = disco.resolve("svc").await.unwrap();
    assert_eq!(resolved.len(), 1);
    assert_eq!(resolved[0].id, "b");
}

#[tokio::test]
async fn resolve_unknown_service_returns_empty() {
    let disco = InMemoryDiscovery::new();
    let resolved = disco.resolve("no-such-service").await.unwrap();

    assert!(resolved.is_empty());
}

// ── Load balancers ──────────────────────────────────────────────────

#[test]
fn round_robin_distributes_across_instances() {
    let rr = RoundRobin::new();
    let instances = vec![instance("a", "svc"), instance("b", "svc"), instance("c", "svc")];

    assert_eq!(rr.pick(&instances).unwrap().id, "a");
    assert_eq!(rr.pick(&instances).unwrap().id, "b");
    assert_eq!(rr.pick(&instances).unwrap().id, "c");
    assert_eq!(rr.pick(&instances).unwrap().id, "a");
}

#[test]
fn random_balancer_returns_valid_instance() {
    let instances = vec![instance("x", "svc"), instance("y", "svc")];
    let picked = Random.pick(&instances);

    assert!(picked.is_some());
    let id = &picked.unwrap().id;
    assert!(id == "x" || id == "y");
}
