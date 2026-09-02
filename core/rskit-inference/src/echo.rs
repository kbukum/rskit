//! Deterministic echo inference adapter.

use async_trait::async_trait;
use rskit_ai::{Capabilities, Model, Provider as ModelProvider, Usage};
use rskit_component::{Component, Health};
use rskit_errors::AppResult;
use rskit_tool::Envelope;

use crate::{
    CapabilityHints, Factory, Inference, InferenceDescriptor, InferenceError, PredictRequest,
    PredictResponse, PredictStatus, Registry, RegistryError, ServingProtocol,
};

/// Registry kind for the echo adapter.
pub const ECHO_KIND: &str = "echo";

/// Echo inference adapter for tests and local composition.
#[derive(Debug, Clone, Default)]
pub struct Echo;

#[async_trait]
impl rskit_provider::Provider for Echo {
    fn name(&self) -> &'static str {
        ECHO_KIND
    }
}

#[async_trait]
impl rskit_provider::RequestResponse<PredictRequest, PredictResponse> for Echo {
    async fn execute(&self, input: PredictRequest) -> AppResult<PredictResponse> {
        self.predict(input).await.map_err(Into::into)
    }
}

#[async_trait]
impl Inference for Echo {
    async fn predict(&self, request: PredictRequest) -> Result<PredictResponse, InferenceError> {
        Ok(PredictResponse {
            outputs: request.inputs,
            usage: Usage::default(),
            model: Model {
                name: request.model_name,
                provider: ModelProvider::Custom("echo".to_string()),
                version: request.model_version,
                capabilities: Capabilities::default(),
            },
            status: PredictStatus::Success,
            reason: None,
            metadata: Default::default(),
        })
    }

    fn descriptor(&self) -> InferenceDescriptor {
        InferenceDescriptor {
            name: ECHO_KIND.to_string(),
            description: "Echo inputs unchanged for tests".to_string(),
            serving_protocol: ServingProtocol::Custom,
            capabilities: CapabilityHints::default(),
            available: true,
            envelope: Envelope::default(),
        }
    }
}

/// Explicitly register the echo adapter.
pub fn register(registry: &mut Registry) -> Result<(), RegistryError> {
    let factory: Factory = std::sync::Arc::new(|| Ok(std::sync::Arc::new(Echo)));
    registry.register(ECHO_KIND, factory)
}

#[async_trait]
impl Component for Echo {
    fn name(&self) -> &str {
        "rskit-inference.echo"
    }

    async fn start(&self) -> rskit_errors::AppResult<()> {
        Ok(())
    }

    async fn stop(&self) -> rskit_errors::AppResult<()> {
        Ok(())
    }

    fn health(&self) -> Health {
        Health::healthy(self.name())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Value;
    use std::collections::HashMap;

    #[tokio::test]
    async fn echo_returns_inputs_unchanged() {
        let adapter = Echo;
        let inputs = HashMap::from([(
            "text".to_string(),
            Value::Text {
                text: "hello".to_string(),
            },
        )]);
        let response = adapter
            .predict(PredictRequest {
                model_name: "echo-model".to_string(),
                inputs: inputs.clone(),
                ..PredictRequest::default()
            })
            .await
            .expect("predict");
        assert_eq!(response.outputs, inputs);
        assert_eq!(response.usage, Usage::default());
        assert_eq!(response.model.name, "echo-model");
        assert_eq!(response.status, PredictStatus::Success);
    }

    #[test]
    fn register_adds_echo_kind() {
        let mut registry = Registry::new();
        register(&mut registry).expect("register echo");
        assert_eq!(registry.kinds(), vec![ECHO_KIND.to_string()]);
    }
}
