use crate::*;
use rskit_component::Component;
use rskit_inference::{
    Inference, InferenceError, PredictRequest, Registry, ServingProtocol, StreamingInference, Value,
};
use rskit_provider::RequestResponse as _;
use std::collections::HashMap;

#[test]
fn vllm_descriptor() {
    let adapter = VllmAdapter::new(Config {
        base_url: "http://localhost:8000".into(),
        model: "llama3".into(),
        api_key: None,
        max_tokens: 256,
    })
    .unwrap();
    let desc = adapter.descriptor();
    assert_eq!(desc.name, VLLM_KIND);
    assert_eq!(desc.serving_protocol, ServingProtocol::VllmRest);
}

#[test]
fn register_adds_vllm_kind() {
    let mut registry = Registry::new();
    register(
        &mut registry,
        Config {
            base_url: "http://localhost:8000".into(),
            model: "llama3".into(),
            api_key: None,
            max_tokens: 256,
        },
    )
    .expect("register vllm");
    assert!(registry.kinds().contains(&VLLM_KIND.to_string()));
}

#[test]
fn config_defaults() {
    let config: Config = serde_json::from_str(r#"{"base_url":"http://localhost:8000"}"#).unwrap();
    assert_eq!(config.model, "default");
    assert_eq!(config.max_tokens, 256);
    assert!(config.api_key.is_none());
}

#[test]
fn completion_body_uses_request_overrides() {
    let config = Config {
        base_url: "http://localhost:8000".into(),
        model: "configured".into(),
        api_key: Some(rskit_util::SecretString::new("secret")),
        max_tokens: 64,
    };
    let mut req = PredictRequest {
        model_name: "requested".to_owned(),
        inputs: HashMap::from([(
            "prompt".to_owned(),
            Value::Text {
                text: "write".to_owned(),
            },
        )]),
        ..PredictRequest::default()
    };
    req.parameters
        .insert("max_tokens".to_owned(), serde_json::json!(9));
    req.parameters
        .insert("temperature".to_owned(), serde_json::json!(0.5));

    let body = serde_json::to_value(vllm_completion_body(&config, &req)).unwrap();

    assert_eq!(body["model"], "requested");
    assert_eq!(body["prompt"], "write");
    assert_eq!(body["max_tokens"], 9);
    assert_eq!(body["temperature"], 0.5);
    assert_eq!(body["stream"], false);
}

#[test]
fn response_mapping_preserves_text_usage_and_finish_reason() {
    let response = vllm_predict_response(
        OaiCompletionResponse {
            model: "served".to_owned(),
            choices: vec![OaiChoice {
                text: "answer".to_owned(),
                finish_reason: Some("length".to_owned()),
            }],
            usage: OaiUsage {
                prompt_tokens: 4,
                completion_tokens: 5,
            },
        },
        Some("rev".to_owned()),
    );

    assert!(matches!(
        response.outputs.get("text"),
        Some(Value::Text { text }) if text == "answer"
    ));
    assert_eq!(
        response.metadata.get("finish_reason").map(String::as_str),
        Some("length")
    );
    assert_eq!(response.usage.output_tokens, 5);
    assert_eq!(response.model.version.as_deref(), Some("rev"));

    let empty = vllm_predict_response(
        OaiCompletionResponse {
            model: "served".to_owned(),
            choices: Vec::new(),
            usage: OaiUsage {
                prompt_tokens: 0,
                completion_tokens: 0,
            },
        },
        None,
    );
    assert!(matches!(
        empty.outputs.get("text"),
        Some(Value::Text { text }) if text.is_empty()
    ));
    assert!(empty.metadata.is_empty());
}

#[tokio::test]
async fn provider_component_streaming_and_execute_fast_paths() {
    let adapter = VllmAdapter::new(Config {
        base_url: "http://127.0.0.1:1".into(),
        model: "test".into(),
        api_key: None,
        max_tokens: 64,
    })
    .unwrap();

    assert_eq!(rskit_provider::Provider::name(&adapter), VLLM_KIND);
    assert_eq!(Component::name(&adapter), "rskit-inference.vllm");
    adapter.start().await.unwrap();
    adapter.stop().await.unwrap();
    assert!(adapter.health().is_healthy());
    assert!(matches!(
        adapter.predict_stream(PredictRequest::default()).await,
        Err(InferenceError::NotImplemented(_))
    ));
    let err = adapter
        .execute(PredictRequest::default())
        .await
        .unwrap_err();
    assert!(matches!(
        err.code(),
        rskit_errors::ErrorCode::ExternalService | rskit_errors::ErrorCode::Internal
    ));
}

#[tokio::test]
async fn predict_extracts_text_input() {
    let adapter = VllmAdapter::new(Config {
        base_url: "http://127.0.0.1:1".into(),
        model: "test".into(),
        api_key: None,
        max_tokens: 64,
    })
    .unwrap();

    let inputs = HashMap::from([("prompt".to_string(), Value::Text { text: "hi".into() })]);
    let req = PredictRequest {
        model_name: "test".into(),
        inputs,
        ..PredictRequest::default()
    };

    let err = adapter.predict(req).await.unwrap_err();
    assert!(
        matches!(
            err,
            InferenceError::Transport(_)
                | InferenceError::Server { .. }
                | InferenceError::Policy(_)
        ),
        "unexpected err: {err:?}"
    );
}
