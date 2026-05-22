use rskit_inference::{PredictRequest, ServingProtocol};

#[tokio::test]
async fn adapter_registers_and_reports_transport_errors() {
    let mut registry = rskit_inference::Registry::new();
    rskit_inference_tgi::register(&mut registry).expect("register adapter");
    assert_eq!(registry.kinds(), vec!["tgi".to_owned()]);

    let adapter = registry
        .build(
            "tgi",
            serde_json::json!({
                "base_url": "http://127.0.0.1:1",
                "model": "test",
                "max_tokens": 8
            }),
        )
        .expect("build adapter");
    assert_eq!(
        adapter.descriptor().serving_protocol,
        ServingProtocol::TgiRest
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
