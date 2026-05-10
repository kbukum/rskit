use std::sync::Arc;

use async_trait::async_trait;
use rskit_inference::{
    Factory, Inference, InferenceDescriptor, InferenceError, PredictRequest, PredictResponse,
    RegistryError, ServingProtocol,
};
use rskit_tool::Envelope;

struct FakeInference;

#[async_trait]
impl rskit_provider::Provider for FakeInference {
    fn name(&self) -> &'static str {
        "fake"
    }
}

#[async_trait]
impl rskit_provider::RequestResponse<PredictRequest, PredictResponse> for FakeInference {
    async fn execute(&self, input: PredictRequest) -> rskit_errors::AppResult<PredictResponse> {
        self.predict(input).await.map_err(Into::into)
    }
}

#[async_trait]
impl Inference for FakeInference {
    async fn predict(&self, _request: PredictRequest) -> Result<PredictResponse, InferenceError> {
        Ok(PredictResponse::default())
    }

    fn descriptor(&self) -> InferenceDescriptor {
        InferenceDescriptor {
            name: "fake".to_owned(),
            description: "fake adapter".to_owned(),
            serving_protocol: ServingProtocol::Custom,
            envelope: Envelope::default(),
        }
    }
}

fn fake_factory() -> Factory {
    Arc::new(|_config| Ok(Arc::new(FakeInference)))
}

#[test]
fn registers_and_builds_adapter() {
    let mut registry = rskit_inference::Registry::new();
    registry
        .register(" fake ", fake_factory())
        .expect("register fake adapter");

    assert_eq!(registry.kinds(), vec!["fake".to_owned()]);
    let adapter = registry
        .build(" fake ", serde_json::Value::Null)
        .expect("build fake adapter");
    assert_eq!(adapter.descriptor().name, "fake");
}

#[test]
fn rejects_duplicate_and_empty_kinds() {
    let mut registry = rskit_inference::Registry::new();
    assert_eq!(
        registry
            .register(" ", fake_factory())
            .expect_err("empty kind"),
        RegistryError::EmptyKind
    );

    registry
        .register("fake", fake_factory())
        .expect("register fake adapter");
    assert_eq!(
        registry
            .register("fake", fake_factory())
            .expect_err("duplicate kind"),
        RegistryError::DuplicateKind("fake".to_owned())
    );
}

#[test]
fn unknown_kind_returns_inference_error() {
    let registry = rskit_inference::Registry::new();
    let err = match registry.build("missing", serde_json::Value::Null) {
        Ok(_) => panic!("unknown kind unexpectedly built"),
        Err(err) => err,
    };
    assert!(err.to_string().contains("unknown inference adapter"));
}

#[test]
fn default_registry_is_empty_convenience() {
    let registry = rskit_inference::default_registry();
    assert!(registry.kinds().is_empty());
}
