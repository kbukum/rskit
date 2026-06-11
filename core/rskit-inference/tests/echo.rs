use std::collections::HashMap;

use rskit_ai::{Capabilities, Provider as ModelProvider, Usage};
use rskit_component::Component;
use rskit_inference::{
    Echo, Inference, PredictRequest, PredictStatus, ServingProtocol, Value, register_echo,
};
use rskit_provider::{Provider, RequestResponse};

#[tokio::test]
async fn echo_predict_preserves_request_inputs_and_model_metadata() {
    let adapter = Echo;
    let inputs = HashMap::from([
        (
            "prompt".to_owned(),
            Value::Text {
                text: "hello".to_owned(),
            },
        ),
        (
            "params".to_owned(),
            Value::Json {
                json: serde_json::json!({"temperature": 0.1}),
            },
        ),
    ]);

    let response = adapter
        .predict(PredictRequest {
            request_id: Some("request-1".to_owned()),
            model_name: "local-echo".to_owned(),
            model_version: Some("v1".to_owned()),
            inputs: inputs.clone(),
            parameters: HashMap::from([("max_tokens".to_owned(), serde_json::json!(16))]),
            metadata: HashMap::from([("tenant".to_owned(), "test".to_owned())]),
            ..PredictRequest::default()
        })
        .await
        .expect("echo prediction should succeed");

    assert_eq!(response.outputs, inputs);
    assert_eq!(response.usage, Usage::default());
    assert_eq!(response.model.name, "local-echo");
    assert_eq!(
        response.model.provider,
        ModelProvider::Custom("echo".to_owned())
    );
    assert_eq!(response.model.version.as_deref(), Some("v1"));
    assert_eq!(response.model.capabilities, Capabilities::default());
    assert_eq!(response.status, PredictStatus::Success);
    assert!(response.metadata.is_empty());
}

#[tokio::test]
async fn echo_request_response_bridge_delegates_to_predict() {
    let adapter = Echo;

    let response = adapter
        .execute(PredictRequest {
            model_name: "bridge".to_owned(),
            inputs: HashMap::from([(
                "text".to_owned(),
                Value::Text {
                    text: "through provider".to_owned(),
                },
            )]),
            ..PredictRequest::default()
        })
        .await
        .expect("request-response bridge should succeed");

    assert_eq!(response.model.name, "bridge");
    assert!(matches!(
        response.outputs.get("text"),
        Some(Value::Text { text }) if text == "through provider"
    ));
}

#[tokio::test]
async fn echo_component_lifecycle_is_healthy_noop() {
    let adapter = Echo;

    adapter.start().await.expect("start should succeed");
    let health = adapter.health();
    adapter.stop().await.expect("stop should succeed");

    assert_eq!(Provider::name(&adapter), "echo");
    assert_eq!(Component::name(&adapter), "rskit-inference.echo");
    assert_eq!(health.name, "rskit-inference.echo");
    assert!(health.is_healthy());
    assert!(health.message.is_none());
}

#[test]
fn echo_descriptor_declares_custom_protocol_and_empty_envelope() {
    let descriptor = Echo.descriptor();

    assert_eq!(descriptor.name, "echo");
    assert_eq!(descriptor.description, "Echo inputs unchanged for tests");
    assert_eq!(descriptor.serving_protocol, ServingProtocol::Custom);
    assert_eq!(descriptor.envelope, rskit_tool::Envelope::default());
}

#[test]
fn register_echo_factory_builds_provider() {
    let mut registry = rskit_inference::Registry::new();

    register_echo(&mut registry).expect("register echo");
    let adapter = registry.build(" echo ").expect("build echo");

    assert_eq!(registry.kinds(), vec!["echo".to_owned()]);
    assert_eq!(adapter.descriptor().name, "echo");
}
