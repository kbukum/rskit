use rskit_inference::{PredictRequest, ServingProtocol};

#[tokio::test]
async fn adapter_registers_and_reports_transport_errors() {
    let mut registry = rskit_inference::Registry::new();
    rskit_inference_vllm::register(
        &mut registry,
        rskit_inference_vllm::Config {
            base_url: "http://127.0.0.1:1".into(),
            model: "test".into(),
            api_key: None,
            max_tokens: 8,
        },
    )
    .expect("register adapter");
    assert_eq!(registry.kinds(), vec!["vllm".to_owned()]);

    let adapter = registry.build("vllm").expect("build adapter");
    assert_eq!(
        adapter.descriptor().serving_protocol,
        ServingProtocol::VllmRest
    );
    let err = match adapter.predict(PredictRequest::default()).await {
        Ok(_) => panic!("adapter unexpectedly predicted"),
        Err(err) => err,
    };
    assert!(
        matches!(
            err,
            rskit_inference::InferenceError::Transport(_)
                | rskit_inference::InferenceError::Server { .. }
                | rskit_inference::InferenceError::Policy(_)
        ),
        "unexpected err: {err:?}"
    );
}
