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
    /// Relative load-balancing weight. Zero is treated as one by weighted balancers.
    #[serde(default = "default_weight")]
    pub weight: u32,
    /// Freeform tags for filtering (e.g. `["canary", "us-east-1"]`).
    pub tags: Vec<String>,
    /// Arbitrary key-value metadata.
    pub metadata: HashMap<String, String>,
}

fn default_weight() -> u32 {
    1
}

impl ServiceInstance {
    /// Returns `"address:port"`.
    pub fn endpoint(&self) -> String {
        format!("{}:{}", self.address, self.port)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn missing_weight_deserializes_to_default() {
        let instance: ServiceInstance = serde_json::from_str(
            r#"{
                "id":"a",
                "name":"svc",
                "address":"127.0.0.1",
                "port":8080,
                "healthy":true,
                "tags":[],
                "metadata":{}
            }"#,
        )
        .unwrap();

        assert_eq!(instance.weight, 1);
    }
}
