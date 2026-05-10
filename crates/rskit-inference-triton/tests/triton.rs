use std::collections::HashMap;

use base64::{Engine, engine::general_purpose::STANDARD as BASE64_STANDARD};
use bytes::Bytes;
use rskit_inference::{Inference, PredictRequest, Tensor, TensorData, Value};
use wiremock::{
    Mock, MockServer, ResponseTemplate,
    matchers::{method, path},
};

#[tokio::test]
async fn predict_happy_path_round_trips_kserve_v2() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/v2/models/demo/infer"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "model_name": "demo",
            "outputs": [
                {"name": "scores", "shape": [2], "datatype": "FP32", "data": [0.25, 0.75]},
                {"name": "ids", "shape": [2], "datatype": "INT64", "data": [10, 20]},
                {
                    "name": "label",
                    "shape": [1],
                    "datatype": "BYTES",
                    "data": [BASE64_STANDARD.encode("cat")],
                    "parameters": {"content_encoding": "base64"}
                }
            ],
            "parameters": {
                "input_tokens": 3,
                "output_tokens": 5,
                "cached_tokens": 1,
                "compute_millis": 7
            }
        })))
        .mount(&server)
        .await;

    let adapter =
        rskit_inference_triton::TritonInference::new(rskit_inference_triton::TritonConfig {
            base_url: server.uri(),
            network_host: "127.0.0.1".to_owned(),
            network_port: None,
            network_scheme: "http".to_owned(),
            ..rskit_inference_triton::TritonConfig::default()
        })
        .expect("adapter constructs");

    let mut inputs = HashMap::new();
    inputs.insert(
        "tokens".to_owned(),
        Value::Tensor {
            tensor: Tensor {
                dtype: "INT64".to_owned(),
                shape: vec![2],
                data: TensorData::I64(vec![1, 2]),
            },
        },
    );
    inputs.insert(
        "prompt".to_owned(),
        Value::Text {
            text: "hello".to_owned(),
        },
    );

    let response = adapter
        .predict(PredictRequest {
            model_name: "demo".to_owned(),
            inputs,
            ..PredictRequest::default()
        })
        .await
        .expect("predict succeeds");

    assert_eq!(response.usage.input_tokens, 3);
    assert_eq!(response.usage.output_tokens, 5);
    assert_eq!(response.usage.cached_tokens, 1);
    assert!(matches!(
        response.outputs.get("scores"),
        Some(Value::Tensor { tensor }) if tensor.data == TensorData::F32(vec![0.25, 0.75])
    ));
    assert!(matches!(
        response.outputs.get("ids"),
        Some(Value::Tensor { tensor }) if tensor.data == TensorData::I64(vec![10, 20])
    ));
    assert!(matches!(
        response.outputs.get("label"),
        Some(Value::Tensor { tensor }) if tensor.data == TensorData::Bytes(vec![Bytes::from_static(b"cat")])
    ));
}

#[tokio::test]
async fn predict_error_response_returns_server_error() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/v2/models/demo/infer"))
        .respond_with(ResponseTemplate::new(503).set_body_string("not ready"))
        .mount(&server)
        .await;

    let adapter =
        rskit_inference_triton::TritonInference::new(rskit_inference_triton::TritonConfig {
            base_url: server.uri(),
            ..rskit_inference_triton::TritonConfig::default()
        })
        .expect("adapter constructs");

    let err = adapter
        .predict(PredictRequest {
            model_name: "demo".to_owned(),
            ..PredictRequest::default()
        })
        .await
        .expect_err("server error");
    assert!(err.to_string().contains("status=503"));
}

#[tokio::test]
async fn health_probe_reports_ready() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/v2/health/ready"))
        .respond_with(ResponseTemplate::new(200))
        .mount(&server)
        .await;

    let adapter =
        rskit_inference_triton::TritonInference::new(rskit_inference_triton::TritonConfig {
            base_url: server.uri(),
            ..rskit_inference_triton::TritonConfig::default()
        })
        .expect("adapter constructs");

    assert!(adapter.health_check().await.expect("health probe"));
}
