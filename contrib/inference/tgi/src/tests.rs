use crate::*;
use rskit_component::Component;
use rskit_inference::{
    Inference, InferenceError, PredictRequest, Registry, ServingProtocol, StreamingInference, Value,
};
use rskit_provider::RequestResponse as _;

#[test]
fn tgi_descriptor() {
    let adapter = TgiAdapter::new(Config {
        base_url: "http://localhost:8080".into(),
        model: "tiiuae/falcon-7b".into(),
        api_key: None,
        max_tokens: 256,
    })
    .unwrap();
    let desc = adapter.descriptor();
    assert_eq!(desc.name, TGI_KIND);
    assert_eq!(desc.serving_protocol, ServingProtocol::TgiRest);
}

#[test]
fn register_adds_tgi_kind() {
    let mut registry = Registry::new();
    register(
        &mut registry,
        Config {
            base_url: "http://localhost:8080".into(),
            model: "tiiuae/falcon-7b".into(),
            api_key: None,
            max_tokens: 256,
        },
    )
    .expect("register tgi");
    assert!(registry.kinds().contains(&TGI_KIND.to_string()));
}

#[test]
fn config_defaults() {
    let config: Config = serde_json::from_str(r#"{"base_url":"http://localhost:8080"}"#).unwrap();
    assert_eq!(config.model, "tgi");
    assert_eq!(config.max_tokens, 256);
    assert!(config.api_key.is_none());
}

#[test]
fn request_body_uses_text_alias_defaults_and_parameters() {
    let config = Config {
        base_url: "http://localhost:8080".into(),
        model: "default-model".into(),
        api_key: Some(rskit_util::SecretString::new("secret")),
        max_tokens: 64,
    };
    let mut req = PredictRequest {
        model_name: String::new(),
        inputs: std::collections::HashMap::from([(
            "text".to_owned(),
            Value::Text {
                text: "hello".to_owned(),
            },
        )]),
        ..PredictRequest::default()
    };
    req.parameters
        .insert("max_tokens".to_owned(), serde_json::json!(7));
    req.parameters
        .insert("temperature".to_owned(), serde_json::json!(0.25));

    let body = serde_json::to_value(tgi_chat_body(&config, &req)).unwrap();

    assert_eq!(body["model"], "default-model");
    assert_eq!(body["messages"][0]["role"], "user");
    assert_eq!(body["messages"][0]["content"], "hello");
    assert_eq!(body["max_tokens"], 7);
    assert_eq!(body["temperature"], 0.25);
    assert_eq!(body["stream"], false);
}

#[test]
fn response_mapping_handles_finish_reason_and_empty_choices() {
    let response = tgi_predict_response(
        OaiChatResponse {
            model: "served".to_owned(),
            choices: vec![OaiChatChoice {
                message: OaiChatMessage {
                    content: Some("done".to_owned()),
                },
                finish_reason: Some("stop".to_owned()),
            }],
            usage: OaiUsage {
                prompt_tokens: 2,
                completion_tokens: 3,
            },
        },
        Some("v1".to_owned()),
    );

    assert!(matches!(
        response.outputs.get("text"),
        Some(Value::Text { text }) if text == "done"
    ));
    assert_eq!(
        response.metadata.get("finish_reason").map(String::as_str),
        Some("stop")
    );
    assert_eq!(response.usage.input_tokens, 2);
    assert_eq!(response.model.version.as_deref(), Some("v1"));

    let empty = tgi_predict_response(
        OaiChatResponse {
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
    let adapter = TgiAdapter::new(Config {
        base_url: "http://127.0.0.1:1".into(),
        model: "test".into(),
        api_key: None,
        max_tokens: 64,
    })
    .unwrap();

    assert_eq!(rskit_provider::Provider::name(&adapter), TGI_KIND);
    assert_eq!(Component::name(&adapter), "rskit-inference.tgi");
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
async fn predict_transport_error_on_bad_url() {
    let adapter = TgiAdapter::new(Config {
        base_url: "http://127.0.0.1:1".into(),
        model: "test".into(),
        api_key: None,
        max_tokens: 64,
    })
    .unwrap();

    let req = PredictRequest {
        model_name: "test".into(),
        inputs: std::collections::HashMap::from([(
            "prompt".to_string(),
            Value::Text {
                text: "hello".into(),
            },
        )]),
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
