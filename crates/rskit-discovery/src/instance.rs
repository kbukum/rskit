use std::collections::HashMap;

use serde::{Deserialize, Serialize};

/// A single instance of a service available for discovery.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ServiceInstance {
    /// Unique instance identifier (e.g. UUID).
    pub id: String,
    /// Logical service name (e.g. `"payment-service"`).
    pub name: String,
    /// Host or IP address.
    pub address: String,
    /// Port the service is listening on.
    pub port: u16,
    /// Whether the instance is currently healthy.
    pub healthy: bool,
    /// Freeform tags for filtering (e.g. `["canary", "us-east-1"]`).
    pub tags: Vec<String>,
    /// Arbitrary key-value metadata.
    pub metadata: HashMap<String, String>,
}

impl ServiceInstance {
    /// Returns `"address:port"`.
    pub fn endpoint(&self) -> String {
        format!("{}:{}", self.address, self.port)
    }
}
