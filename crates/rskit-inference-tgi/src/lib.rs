//! Hugging Face TGI REST adapter skeleton for `rskit-inference`.
//!
//! Implementation pending; PRs welcome. The crate exists to lock the backend
//! split shape: explicit registration, descriptor, and streaming-capable trait
//! surface without auto-registration or globals.

#![warn(missing_docs)]

use std::sync::Arc;

use async_trait::async_trait;
use rskit_inference::{
    Factory, Inference, InferenceDescriptor, InferenceError, PredictRequest, PredictResponse,
    Registry, RegistryError, ServingProtocol, StreamEventRef, StreamingInference,
};
use rskit_tool::Envelope;
use serde::{Deserialize, Serialize};
use tokio_stream::Stream;

/// Registry kind for the TGI adapter skeleton.
pub const KIND: &str = "tgi";

/// Configuration for the Hugging Face TGI REST adapter skeleton.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Config {
    /// Adapter endpoint base URL.
    #[serde(default = "default_endpoint")]
    pub endpoint: String,
    /// Descriptor name.
    #[serde(default = "default_name")]
    pub name: String,
    /// Descriptor description.
    #[serde(default = "default_description")]
    pub description: String,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            endpoint: default_endpoint(),
            name: default_name(),
            description: default_description(),
        }
    }
}

/// Hugging Face TGI REST adapter skeleton.
pub struct Adapter {
    config: Config,
    client: reqwest::Client,
}

impl Adapter {
    /// Create a skeleton adapter with an injected HTTP client.
    #[must_use]
    pub fn new(config: Config, client: reqwest::Client) -> Self {
        Self { config, client }
    }
}

#[async_trait]
impl Inference for Adapter {
    async fn predict(&self, _request: PredictRequest) -> Result<PredictResponse, InferenceError> {
        let _client = &self.client;
        Err(InferenceError::NotImplemented(
            "Hugging Face TGI REST adapter implementation pending; PRs welcome",
        ))
    }

    fn descriptor(&self) -> InferenceDescriptor {
        InferenceDescriptor {
            name: self.config.name.clone(),
            description: self.config.description.clone(),
            serving_protocol: ServingProtocol::TgiRest,
            envelope: Envelope {
                scopes: vec!["inference:predict".to_owned()],
                ..Envelope::default()
            },
        }
    }
}

#[async_trait]
impl StreamingInference for Adapter {
    async fn predict_stream(
        &self,
        _request: PredictRequest,
    ) -> Result<Box<dyn Stream<Item = StreamEventRef> + Send + Unpin>, InferenceError> {
        let _client = &self.client;
        Err(InferenceError::NotImplemented(
            "Hugging Face TGI REST streaming implementation pending; PRs welcome",
        ))
    }
}

#[async_trait]
impl rskit_component::Component for Adapter {
    fn name(&self) -> &str {
        &self.config.name
    }

    async fn start(&self) -> rskit_errors::AppResult<()> {
        Ok(())
    }

    async fn stop(&self) -> rskit_errors::AppResult<()> {
        Ok(())
    }

    fn health(&self) -> rskit_component::Health {
        rskit_component::Health::healthy(self.name())
    }
}

/// Register this adapter skeleton in an explicit registry.
pub fn register(registry: &mut Registry) -> Result<(), RegistryError> {
    let factory: Factory = Arc::new(|config| {
        let config = if config.is_null() {
            Config::default()
        } else {
            serde_json::from_value::<Config>(config)
                .map_err(|err| InferenceError::Decode(err.to_string()))?
        };
        Ok(Arc::new(Adapter::new(config, reqwest::Client::new())))
    });
    registry.register(KIND, factory)
}

fn default_endpoint() -> String {
    "http://localhost:8000".to_owned()
}

fn default_name() -> String {
    "tgi".to_owned()
}

fn default_description() -> String {
    "Hugging Face TGI REST adapter skeleton; implementation pending; PRs welcome".to_owned()
}
