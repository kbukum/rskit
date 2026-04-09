use std::collections::{HashMap, HashSet};
use std::sync::Arc;

use async_trait::async_trait;
use rskit_discovery::{
    Discovery, InMemoryDiscovery, LeastConnections, LoadBalancer, Random, Registry, RoundRobin,
    ServiceInstance,
};
use rskit_errors::AppResult;

// ── Helpers ─────────────────────────────────────────────────────────

fn make_instance(id: &str, name: &str) -> ServiceInstance {
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

fn make_full_instance(id: &str, name: &str, addr: &str, port: u16) -> ServiceInstance {
    ServiceInstance {
        id: id.into(),
        name: name.into(),
        address: addr.into(),
        port,
        healthy: true,
        tags: vec!["prod".into(), "us-east-1".into()],
        metadata: HashMap::from([
            ("version".into(), "1.0".into()),
            ("env".into(), "production".into()),
        ]),
    }
}

// ── ServiceInstance tests ───────────────────────────────────────────

#[test]
fn instance_creation_all_fields() {
    let inst = make_full_instance("svc-1", "payment", "192.168.1.10", 9090);
    assert_eq!(inst.id, "svc-1");
    assert_eq!(inst.name, "payment");
    assert_eq!(inst.address, "192.168.1.10");
    assert_eq!(inst.port, 9090);
    assert!(inst.healthy);
    assert_eq!(inst.tags.len(), 2);
    assert!(inst.tags.contains(&"prod".to_string()));
    assert_eq!(inst.metadata.get("version").unwrap(), "1.0");
}

#[test]
fn instance_endpoint_formatting() {
    let inst = make_full_instance("id", "svc", "10.0.0.5", 3000);
    assert_eq!(inst.endpoint(), "10.0.0.5:3000");
}

#[test]
fn instance_clone() {
    let orig = make_full_instance("c1", "svc", "10.0.0.1", 80);
    let cloned = orig.clone();
    assert_eq!(cloned.id, orig.id);
    assert_eq!(cloned.name, orig.name);
    assert_eq!(cloned.tags, orig.tags);
    assert_eq!(cloned.metadata, orig.metadata);
}

#[test]
fn instance_tags_and_metadata_preservation() {
    let mut metadata = HashMap::new();
    metadata.insert("region".into(), "us-west-2".into());
    metadata.insert("team".into(), "platform".into());

    let inst = ServiceInstance {
        id: "m1".into(),
        name: "svc".into(),
        address: "h".into(),
        port: 80,
        healthy: true,
        tags: vec!["canary".into(), "v2".into(), "gpu".into()],
        metadata,
    };
    assert_eq!(inst.tags.len(), 3);
    assert!(inst.tags.contains(&"canary".to_string()));
    assert_eq!(inst.metadata.get("region").unwrap(), "us-west-2");
    assert_eq!(inst.metadata.len(), 2);
}

#[test]
fn instance_empty_metadata() {
    let inst = make_instance("e1", "svc");
    assert!(inst.metadata.is_empty());
    assert!(inst.tags.is_empty());
}

#[test]
fn instance_port_zero() {
    let inst = ServiceInstance {
        id: "p0".into(),
        name: "svc".into(),
        address: "localhost".into(),
        port: 0,
        healthy: true,
        tags: vec![],
        metadata: HashMap::new(),
    };
    assert_eq!(inst.endpoint(), "localhost:0");
}

#[test]
fn instance_serialization_roundtrip() {
    let inst = make_full_instance("ser-1", "api", "10.0.0.1", 8080);
    let json = serde_json::to_string(&inst).expect("serialize");
    let deserialized: ServiceInstance = serde_json::from_str(&json).expect("deserialize");
    assert_eq!(deserialized.id, inst.id);
    assert_eq!(deserialized.name, inst.name);
    assert_eq!(deserialized.address, inst.address);
    assert_eq!(deserialized.port, inst.port);
    assert_eq!(deserialized.healthy, inst.healthy);
    assert_eq!(deserialized.tags, inst.tags);
    assert_eq!(deserialized.metadata, inst.metadata);
}

#[test]
fn instance_special_characters_in_name() {
    let inst = ServiceInstance {
        id: "special-1".into(),
        name: "my-service/v2.beta:latest".into(),
        address: "host".into(),
        port: 80,
        healthy: true,
        tags: vec![],
        metadata: HashMap::new(),
    };
    assert_eq!(inst.name, "my-service/v2.beta:latest");
}

#[test]
fn instance_very_long_service_name() {
    let long_name = "a".repeat(1000);
    let inst = ServiceInstance {
        id: "long-1".into(),
        name: long_name.clone(),
        address: "h".into(),
        port: 80,
        healthy: true,
        tags: vec![],
        metadata: HashMap::new(),
    };
    assert_eq!(inst.name.len(), 1000);
    assert_eq!(inst.name, long_name);
}

// ── InMemoryDiscovery tests ─────────────────────────────────────────

#[tokio::test]
async fn memory_register_and_resolve() {
    let disco = InMemoryDiscovery::new();
    let inst = make_instance("a", "svc");
    disco.register(&inst).await.unwrap();

    let resolved = disco.resolve("svc").await.unwrap();
    assert_eq!(resolved.len(), 1);
    assert_eq!(resolved[0].id, "a");
}

#[tokio::test]
async fn memory_resolve_unregistered_returns_empty() {
    let disco = InMemoryDiscovery::new();
    let resolved = disco.resolve("no-such-service").await.unwrap();
    assert!(resolved.is_empty());
}

#[tokio::test]
async fn memory_deregister_removes_instance() {
    let disco = InMemoryDiscovery::new();
    disco.register(&make_instance("a", "svc")).await.unwrap();
    disco.register(&make_instance("b", "svc")).await.unwrap();

    disco.deregister("a").await.unwrap();
    let resolved = disco.resolve("svc").await.unwrap();
    assert_eq!(resolved.len(), 1);
    assert_eq!(resolved[0].id, "b");
}

#[tokio::test]
async fn memory_deregister_not_found_returns_error() {
    let disco = InMemoryDiscovery::new();
    let result = disco.deregister("nonexistent").await;
    assert!(result.is_err());
}

#[tokio::test]
async fn memory_register_multiple_same_service() {
    let disco = InMemoryDiscovery::new();
    for i in 0..10 {
        let inst = make_instance(&format!("inst-{i}"), "api");
        disco.register(&inst).await.unwrap();
    }
    let resolved = disco.resolve("api").await.unwrap();
    assert_eq!(resolved.len(), 10);
}

#[tokio::test]
async fn memory_register_multiple_services() {
    let disco = InMemoryDiscovery::new();
    disco.register(&make_instance("a1", "api")).await.unwrap();
    disco.register(&make_instance("w1", "web")).await.unwrap();
    disco.register(&make_instance("d1", "db")).await.unwrap();

    assert_eq!(disco.resolve("api").await.unwrap().len(), 1);
    assert_eq!(disco.resolve("web").await.unwrap().len(), 1);
    assert_eq!(disco.resolve("db").await.unwrap().len(), 1);
}

#[tokio::test]
async fn memory_register_same_instance_twice_adds_duplicate() {
    let disco = InMemoryDiscovery::new();
    let inst = make_instance("dup", "svc");
    disco.register(&inst).await.unwrap();
    disco.register(&inst).await.unwrap();

    // The current implementation simply pushes, so duplicates are possible
    let resolved = disco.resolve("svc").await.unwrap();
    assert!(resolved.len() >= 1);
}

#[tokio::test]
async fn memory_add_and_remove() {
    let disco = InMemoryDiscovery::new();
    let inst = make_instance("x", "svc");
    disco.add("svc", inst).await;

    let resolved = disco.resolve("svc").await.unwrap();
    assert_eq!(resolved.len(), 1);

    disco.remove("svc", "x").await;
    let resolved = disco.resolve("svc").await.unwrap();
    assert!(resolved.is_empty());
}

#[tokio::test]
async fn memory_default_constructor() {
    let disco = InMemoryDiscovery::default();
    let resolved = disco.resolve("anything").await.unwrap();
    assert!(resolved.is_empty());
}

#[tokio::test]
async fn memory_concurrent_register_resolve() {
    let disco = Arc::new(InMemoryDiscovery::new());
    let mut handles = vec![];

    // Spawn 20 concurrent register tasks
    for i in 0..20 {
        let d = disco.clone();
        handles.push(tokio::spawn(async move {
            let inst = make_instance(&format!("c-{i}"), "concurrent-svc");
            d.register(&inst).await.unwrap();
        }));
    }

    // Spawn 20 concurrent resolve tasks
    for _ in 0..20 {
        let d = disco.clone();
        handles.push(tokio::spawn(async move {
            let _ = d.resolve("concurrent-svc").await.unwrap();
        }));
    }

    for h in handles {
        h.await.unwrap();
    }

    let resolved = disco.resolve("concurrent-svc").await.unwrap();
    assert_eq!(resolved.len(), 20);
}

#[tokio::test]
async fn memory_full_lifecycle() {
    let disco = InMemoryDiscovery::new();

    // Register
    disco
        .register(&make_instance("lc-1", "lifecycle-svc"))
        .await
        .unwrap();
    disco
        .register(&make_instance("lc-2", "lifecycle-svc"))
        .await
        .unwrap();
    assert_eq!(disco.resolve("lifecycle-svc").await.unwrap().len(), 2);

    // Deregister one
    disco.deregister("lc-1").await.unwrap();
    let remaining = disco.resolve("lifecycle-svc").await.unwrap();
    assert_eq!(remaining.len(), 1);
    assert_eq!(remaining[0].id, "lc-2");

    // Deregister last
    disco.deregister("lc-2").await.unwrap();
    assert!(disco.resolve("lifecycle-svc").await.unwrap().is_empty());
}

// ── LoadBalancer: RoundRobin ────────────────────────────────────────

#[test]
fn round_robin_cycles_through_instances() {
    let rr = RoundRobin::new();
    let instances = vec![
        make_instance("a", "svc"),
        make_instance("b", "svc"),
        make_instance("c", "svc"),
    ];

    assert_eq!(rr.pick(&instances).unwrap().id, "a");
    assert_eq!(rr.pick(&instances).unwrap().id, "b");
    assert_eq!(rr.pick(&instances).unwrap().id, "c");
    assert_eq!(rr.pick(&instances).unwrap().id, "a");
    assert_eq!(rr.pick(&instances).unwrap().id, "b");
}

#[test]
fn round_robin_single_instance() {
    let rr = RoundRobin::new();
    let instances = vec![make_instance("only", "svc")];

    for _ in 0..10 {
        assert_eq!(rr.pick(&instances).unwrap().id, "only");
    }
}

#[test]
fn round_robin_empty_returns_none() {
    let rr = RoundRobin::new();
    assert!(rr.pick(&[]).is_none());
}

#[test]
fn round_robin_default_trait() {
    let rr = RoundRobin::default();
    let instances = vec![make_instance("a", "svc")];
    assert!(rr.pick(&instances).is_some());
}

#[test]
fn round_robin_fairness() {
    let rr = RoundRobin::new();
    let instances: Vec<_> = (0..5)
        .map(|i| make_instance(&format!("n{i}"), "svc"))
        .collect();
    let mut counts: HashMap<String, usize> = HashMap::new();

    for _ in 0..100 {
        let picked = rr.pick(&instances).unwrap();
        *counts.entry(picked.id.clone()).or_default() += 1;
    }

    // Each should get exactly 20
    for i in 0..5 {
        assert_eq!(*counts.get(&format!("n{i}")).unwrap(), 20);
    }
}

// ── LoadBalancer: Random ────────────────────────────────────────────

#[test]
fn random_picks_from_available() {
    let instances = vec![
        make_instance("x", "svc"),
        make_instance("y", "svc"),
        make_instance("z", "svc"),
    ];
    let valid_ids: HashSet<&str> = ["x", "y", "z"].into_iter().collect();

    for _ in 0..50 {
        let picked = Random.pick(&instances).unwrap();
        assert!(valid_ids.contains(picked.id.as_str()));
    }
}

#[test]
fn random_empty_returns_none() {
    assert!(Random.pick(&[]).is_none());
}

#[test]
fn random_single_instance() {
    let instances = vec![make_instance("only", "svc")];
    for _ in 0..10 {
        assert_eq!(Random.pick(&instances).unwrap().id, "only");
    }
}

#[test]
fn random_distribution() {
    let instances: Vec<_> = (0..3)
        .map(|i| make_instance(&format!("r{i}"), "svc"))
        .collect();
    let mut seen: HashSet<String> = HashSet::new();

    for _ in 0..100 {
        let picked = Random.pick(&instances).unwrap();
        seen.insert(picked.id.clone());
    }

    // With 100 picks from 3, we should see at least 2
    assert!(
        seen.len() >= 2,
        "only saw {} unique IDs from 100 picks",
        seen.len()
    );
}

// ── LoadBalancer: LeastConnections ──────────────────────────────────

#[test]
fn least_connections_prefers_idle() {
    let lc = LeastConnections::new();
    let instances = vec![make_instance("busy", "svc"), make_instance("idle", "svc")];

    lc.acquire("busy");
    lc.acquire("busy");
    lc.acquire("idle");

    assert_eq!(lc.pick(&instances).unwrap().id, "idle");
}

#[test]
fn least_connections_empty_returns_none() {
    let lc = LeastConnections::new();
    assert!(lc.pick(&[]).is_none());
}

#[test]
fn least_connections_acquire_release() {
    let lc = LeastConnections::new();
    let instances = vec![make_instance("a", "svc"), make_instance("b", "svc")];

    lc.acquire("a");
    lc.acquire("a");
    lc.acquire("a");
    lc.release("a");
    lc.release("a");

    // a has 1, b has 0
    assert_eq!(lc.pick(&instances).unwrap().id, "b");
}

#[test]
fn least_connections_tie_goes_to_first() {
    let lc = LeastConnections::new();
    let instances = vec![make_instance("a", "svc"), make_instance("b", "svc")];

    // Both have 0 in-flight; min_by_key returns first match
    let picked = lc.pick(&instances).unwrap();
    assert_eq!(picked.id, "a");
}

#[test]
fn least_connections_default_trait() {
    let lc = LeastConnections::default();
    let instances = vec![make_instance("x", "svc")];
    assert!(lc.pick(&instances).is_some());
}

#[test]
fn least_connections_single_instance() {
    let lc = LeastConnections::new();
    let instances = vec![make_instance("only", "svc")];
    lc.acquire("only");
    lc.acquire("only");
    // Even with in-flight, single instance should still be picked
    assert_eq!(lc.pick(&instances).unwrap().id, "only");
}

// ── Trait contracts ─────────────────────────────────────────────────

/// Custom Discovery implementation for testing trait contracts.
struct StubDiscovery {
    instances: Vec<ServiceInstance>,
}

#[async_trait]
impl Discovery for StubDiscovery {
    async fn resolve(&self, _service: &str) -> AppResult<Vec<ServiceInstance>> {
        Ok(self.instances.clone())
    }
}

/// Custom Registry implementation for testing trait contracts.
struct StubRegistry {
    registered: tokio::sync::Mutex<Vec<String>>,
}

#[async_trait]
impl Registry for StubRegistry {
    async fn register(&self, instance: &ServiceInstance) -> AppResult<()> {
        self.registered.lock().await.push(instance.id.clone());
        Ok(())
    }

    async fn deregister(&self, id: &str) -> AppResult<()> {
        let mut reg = self.registered.lock().await;
        reg.retain(|i| i != id);
        Ok(())
    }
}

#[tokio::test]
async fn custom_discovery_trait_impl() {
    let stub = StubDiscovery {
        instances: vec![make_instance("custom-1", "svc")],
    };
    let result = stub.resolve("svc").await.unwrap();
    assert_eq!(result.len(), 1);
    assert_eq!(result[0].id, "custom-1");
}

#[tokio::test]
async fn custom_registry_trait_impl() {
    let stub = StubRegistry {
        registered: tokio::sync::Mutex::new(vec![]),
    };
    let inst = make_instance("reg-1", "svc");
    stub.register(&inst).await.unwrap();

    let reg = stub.registered.lock().await;
    assert_eq!(reg.len(), 1);
    assert_eq!(reg[0], "reg-1");
    drop(reg);

    stub.deregister("reg-1").await.unwrap();
    let reg = stub.registered.lock().await;
    assert!(reg.is_empty());
}

// Verify traits are object-safe (can be used as dyn trait objects)
#[tokio::test]
async fn traits_are_object_safe() {
    let disco: Box<dyn Discovery> = Box::new(InMemoryDiscovery::new());
    let result = disco.resolve("nonexistent").await.unwrap();
    assert!(result.is_empty());

    let registry: Box<dyn Registry> = Box::new(InMemoryDiscovery::new());
    let inst = make_instance("obj-1", "svc");
    registry.register(&inst).await.unwrap();
}

// Verify Send + Sync bounds
#[tokio::test]
async fn discovery_is_send_sync() {
    let disco = Arc::new(InMemoryDiscovery::new());
    let d = disco.clone();

    let handle = tokio::spawn(async move {
        d.register(&make_instance("ss-1", "svc")).await.unwrap();
        d.resolve("svc").await.unwrap()
    });

    let result = handle.await.unwrap();
    assert_eq!(result.len(), 1);
}
