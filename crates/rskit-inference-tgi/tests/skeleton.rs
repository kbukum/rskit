use rskit_inference::{PredictRequest, ServingProtocol};

#[tokio::test]
async fn skeleton_registers_and_reports_unimplemented() {
    let mut registry = rskit_inference::Registry::new();
    rskit_inference_tgi::register(&mut registry).expect("register skeleton");
    assert_eq!(registry.kinds(), vec!["tgi".to_owned()]);

    let adapter = registry
        .build("tgi", serde_json::Value::Null)
        .expect("build skeleton");
    assert_eq!(
        adapter.descriptor().serving_protocol,
        ServingProtocol::TgiRest
    );
    let err = match adapter.predict(PredictRequest::default()).await {
        Ok(_) => panic!("skeleton unexpectedly predicted"),
        Err(err) => err,
    };
    assert!(err.to_string().contains("implementation pending"));
}
