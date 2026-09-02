use std::sync::atomic::{AtomicUsize, Ordering};

use dashmap::DashMap;

use crate::instance::ServiceInstance;

/// Strategy for picking one instance from a set of candidates.
pub trait LoadBalancer: Send + Sync {
    /// Select an instance, or `None` if the slice is empty.
    fn pick<'a>(&self, instances: &'a [ServiceInstance]) -> Option<&'a ServiceInstance>;
}

/// Cycles through instances in order.
pub struct RoundRobin {
    counter: AtomicUsize,
}

impl RoundRobin {
    /// Create a new round-robin balancer.
    pub fn new() -> Self {
        Self {
            counter: AtomicUsize::new(0),
        }
    }
}

impl Default for RoundRobin {
    fn default() -> Self {
        Self::new()
    }
}

impl LoadBalancer for RoundRobin {
    fn pick<'a>(&self, instances: &'a [ServiceInstance]) -> Option<&'a ServiceInstance> {
        if instances.is_empty() {
            return None;
        }
        let idx = self.counter.fetch_add(1, Ordering::Relaxed) % instances.len();
        Some(&instances[idx])
    }
}

/// Picks a random instance each time.
pub struct Random;

impl LoadBalancer for Random {
    fn pick<'a>(&self, instances: &'a [ServiceInstance]) -> Option<&'a ServiceInstance> {
        if instances.is_empty() {
            return None;
        }
        use rand::Rng;
        let idx = rand::rng().random_range(0..instances.len());
        Some(&instances[idx])
    }
}

/// Picks an instance randomly according to each instance's relative weight.
#[derive(Debug, Clone, Copy, Default)]
pub struct Weighted;

impl LoadBalancer for Weighted {
    fn pick<'a>(&self, instances: &'a [ServiceInstance]) -> Option<&'a ServiceInstance> {
        if instances.is_empty() {
            return None;
        }

        let total = instances
            .iter()
            .map(|instance| u64::from(instance.weight.max(1)))
            .sum::<u64>();
        use rand::Rng;
        let mut slot = rand::rng().random_range(0..total);
        for instance in instances {
            let weight = u64::from(instance.weight.max(1));
            if slot < weight {
                return Some(instance);
            }
            slot -= weight;
        }
        instances.last()
    }
}

/// Tracks in-flight requests per instance and picks the one with the fewest.
pub struct LeastConnections {
    in_flight: DashMap<String, AtomicUsize>,
}

impl LeastConnections {
    /// Create a new least-connections balancer.
    pub fn new() -> Self {
        Self {
            in_flight: DashMap::new(),
        }
    }

    /// Increment the in-flight count for an instance.
    pub fn acquire(&self, id: &str) {
        self.in_flight
            .entry(id.to_owned())
            .or_insert_with(|| AtomicUsize::new(0))
            .fetch_add(1, Ordering::Relaxed);
    }

    /// Decrement the in-flight count for an instance.
    pub fn release(&self, id: &str) {
        if let Some(counter) = self.in_flight.get(id) {
            counter.fetch_sub(1, Ordering::Relaxed);
        }
    }
}

impl Default for LeastConnections {
    fn default() -> Self {
        Self::new()
    }
}

impl LoadBalancer for LeastConnections {
    fn pick<'a>(&self, instances: &'a [ServiceInstance]) -> Option<&'a ServiceInstance> {
        if instances.is_empty() {
            return None;
        }
        instances.iter().min_by_key(|inst| {
            self.in_flight
                .get(&inst.id)
                .map(|c| c.load(Ordering::Relaxed))
                .unwrap_or(0)
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;

    fn instance(id: &str) -> ServiceInstance {
        ServiceInstance {
            id: id.into(),
            name: "svc".into(),
            address: "127.0.0.1".into(),
            port: 8080,
            protocol: String::new(),
            health: crate::instance::HealthState::Healthy,
            weight: 1,
            tags: vec![],
            metadata: HashMap::new(),
            last_seen: None,
        }
    }

    #[test]
    fn round_robin_cycles() {
        let rr = RoundRobin::new();
        let instances = vec![instance("a"), instance("b"), instance("c")];

        assert_eq!(rr.pick(&instances).unwrap().id, "a");
        assert_eq!(rr.pick(&instances).unwrap().id, "b");
        assert_eq!(rr.pick(&instances).unwrap().id, "c");
        assert_eq!(rr.pick(&instances).unwrap().id, "a");
    }

    #[test]
    fn round_robin_empty() {
        let rr = RoundRobin::new();
        assert!(rr.pick(&[]).is_none());
    }

    #[test]
    fn random_non_empty() {
        let instances = vec![instance("a"), instance("b")];
        let picked = Random.pick(&instances);
        assert!(picked.is_some());
    }

    #[test]
    fn weighted_non_empty() {
        let instances = vec![instance("a"), instance("b")];
        let picked = Weighted.pick(&instances);
        assert!(picked.is_some());
    }

    #[test]
    fn least_connections_prefers_idle() {
        let lc = LeastConnections::new();
        let instances = vec![instance("a"), instance("b")];

        lc.acquire("a");
        lc.acquire("a");
        lc.acquire("b");

        assert_eq!(lc.pick(&instances).unwrap().id, "b");
    }
}
