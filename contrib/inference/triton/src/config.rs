use std::{fmt, sync::Arc};

use rskit_authz::Decider;
use rskit_resilience::Policy;
use serde::{Deserialize, Serialize};

/// Configuration for Triton KServe v2 HTTP serving.
#[derive(Clone, Serialize, Deserialize)]
pub struct Config {
    /// Base URL for the Triton HTTP endpoint.
    #[serde(default = "default_base_url")]
    pub base_url: String,
    /// Descriptor name.
    #[serde(default = "default_name")]
    pub name: String,
    /// Descriptor description.
    #[serde(default = "default_description")]
    pub description: String,
    /// Network host declared in the executable envelope.
    #[serde(default = "default_network_host")]
    pub network_host: String,
    /// Network port declared in the executable envelope.
    #[serde(default = "default_network_port")]
    pub network_port: Option<u16>,
    /// Network scheme declared in the executable envelope.
    #[serde(default = "default_network_scheme")]
    pub network_scheme: String,
    /// Authz scopes declared in the executable envelope.
    #[serde(default = "default_scopes")]
    pub scopes: Vec<String>,
    /// Optional resilience policy applied around prediction requests.
    #[serde(skip)]
    pub policy: Option<Policy>,
    /// Optional authorization decider evaluated before prediction requests.
    #[serde(skip)]
    pub decider: Option<Arc<dyn Decider>>,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            base_url: default_base_url(),
            name: default_name(),
            description: default_description(),
            network_host: default_network_host(),
            network_port: default_network_port(),
            network_scheme: default_network_scheme(),
            scopes: default_scopes(),
            policy: None,
            decider: None,
        }
    }
}

impl Config {
    /// Configure a resilience policy for prediction requests.
    #[must_use]
    pub fn with_policy(mut self, policy: Policy) -> Self {
        self.policy = Some(policy);
        self
    }

    /// Configure an authorization decider for prediction requests.
    #[must_use]
    pub fn with_decider(mut self, decider: Arc<dyn Decider>) -> Self {
        self.decider = Some(decider);
        self
    }
}

impl fmt::Debug for Config {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Config")
            .field("base_url", &self.base_url)
            .field("name", &self.name)
            .field("description", &self.description)
            .field("network_host", &self.network_host)
            .field("network_port", &self.network_port)
            .field("network_scheme", &self.network_scheme)
            .field("scopes", &self.scopes)
            .field("policy", &self.policy.as_ref().map(|_| "<configured>"))
            .field("decider", &self.decider.as_ref().map(|_| "<configured>"))
            .finish()
    }
}

fn default_base_url() -> String {
    "http://localhost:8000".to_owned()
}

fn default_name() -> String {
    "triton".to_owned()
}

fn default_description() -> String {
    "Triton KServe v2 model-serving adapter".to_owned()
}

fn default_network_host() -> String {
    "localhost".to_owned()
}

fn default_network_port() -> Option<u16> {
    Some(8000)
}

fn default_network_scheme() -> String {
    "http".to_owned()
}

fn default_scopes() -> Vec<String> {
    vec!["inference:predict".to_owned()]
}
