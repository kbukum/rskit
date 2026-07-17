use std::{collections::HashMap, sync::Arc};

use bytes::Bytes;
use rskit_ai::semconv::Operation;
use rskit_authz::{AuthzDecision, AuthzRequest, Decider};
use rskit_inference::{
    Inference, InferenceError, PredictRequest, PredictStatus, Registry, ServingProtocol, Tensor,
    TensorData, Value,
};
use rskit_resilience::Policy;
use rskit_tool::NetworkPolicy;

use crate::*;

struct DenyDecider;

impl Decider for DenyDecider {
    fn decide(&self, _request: &AuthzRequest) -> AuthzDecision {
        AuthzDecision::Deny("no".to_owned())
    }
}

#[test]
fn config_defaults_and_descriptor_are_declared() {
    let config = Config::default();
    let descriptor = descriptor_from_config(&config);
    assert_eq!(descriptor.name, "triton");
    assert_eq!(descriptor.serving_protocol, ServingProtocol::KServeV2Http);
    assert_eq!(
        descriptor.envelope.scopes,
        vec!["inference:predict".to_owned()]
    );
    assert!(matches!(
        descriptor.envelope.network,
        NetworkPolicy::AllowList { .. }
    ));
}

#[test]
fn infer_path_handles_version_and_rejects_invalid_segments() {
    assert_eq!(
        infer_path("model", Some("1")).expect("versioned path"),
        "/v2/models/model/versions/1/infer"
    );
    assert!(infer_path("bad/name", None).is_err());
    assert!(infer_path("model", Some("")).is_err());
}

#[test]
fn operation_name_defaults_and_uses_parameter() {
    let mut request = PredictRequest::default();
    assert_eq!(operation_name(&request), Operation::InferenceRequest);
    assert_eq!(operation_name(&request).as_str(), "inference.request");
    request
        .parameters
        .insert("operation".to_owned(), serde_json::json!("embedding"));
    assert_eq!(operation_name(&request), Operation::Embedding);
}

#[test]
fn encode_request_covers_value_variants() {
    let mut request = PredictRequest {
        model_name: "model".to_owned(),
        ..PredictRequest::default()
    };
    request.inputs.insert(
        "text".to_owned(),
        Value::Text {
            text: "hello".to_owned(),
        },
    );
    request.inputs.insert(
        "bytes".to_owned(),
        Value::Bytes {
            bytes: Bytes::from_static(b"hello"),
        },
    );
    request.inputs.insert(
        "json".to_owned(),
        Value::Json {
            json: serde_json::json!({"a": 1}),
        },
    );
    request.inputs.insert(
        "fp32".to_owned(),
        Value::Tensor {
            tensor: Tensor {
                dtype: "FP32".to_owned(),
                shape: vec![2],
                data: TensorData::F32(vec![1.0, 2.0]),
            },
        },
    );
    request.inputs.insert(
        "bytes_tensor".to_owned(),
        Value::Tensor {
            tensor: Tensor {
                dtype: "BYTES".to_owned(),
                shape: vec![1],
                data: TensorData::Bytes(vec![Bytes::from_static(b"x")]),
            },
        },
    );
    request.request_id = Some("req-123".to_owned());
    request
        .parameters
        .insert("top_k".to_owned(), serde_json::json!(3));
    request
        .parameters
        .insert("temperature".to_owned(), serde_json::json!(0.7));
    request.options = serde_json::json!({
        "temperature": 1.0,
        "max_new_tokens": 16
    });

    let encoded = encode_request(&request).expect("encode request");
    assert!(
        encoded
            .get("inputs")
            .and_then(serde_json::Value::as_array)
            .is_some()
    );
    assert_eq!(encoded["id"], "req-123");
    assert_eq!(encoded["parameters"]["top_k"], 3);
    assert_eq!(encoded["parameters"]["max_new_tokens"], 16);
    assert_eq!(encoded["parameters"]["temperature"], 0.7);
}

#[test]
fn encode_tensor_rejects_unsupported_dtype_or_data_pair() {
    let tensor = Tensor {
        dtype: "FP32".to_owned(),
        shape: vec![1],
        data: TensorData::I64(vec![1]),
    };
    assert!(encode_tensor("bad", &tensor).is_err());
}

#[test]
fn decode_response_covers_metadata_usage_and_raw_bytes() {
    let response = TritonResponse {
        outputs: vec![TritonOutput {
            name: "raw".to_owned(),
            shape: vec![1],
            datatype: "BYTES".to_owned(),
            data: serde_json::json!(["plain"]),
            parameters: None,
        }],
        parameters: Some(HashMap::from([
            ("input_tokens".to_owned(), serde_json::json!(1)),
            ("output_tokens".to_owned(), serde_json::json!(2)),
            ("cached_tokens".to_owned(), serde_json::json!(3)),
            ("compute_millis".to_owned(), serde_json::json!(4)),
        ])),
        model_name: Some("model".to_owned()),
        model_version: Some("1".to_owned()),
    };
    let decoded = decode_response(response).expect("decode response");
    assert_eq!(decoded.metadata["model_name"], "model");
    assert_eq!(decoded.model.name, "model");
    assert_eq!(decoded.model.version.as_deref(), Some("1"));
    assert_eq!(decoded.status, PredictStatus::Success);
    assert_eq!(decoded.usage.reasoning_tokens, 0);
    assert!(matches!(
        decoded.outputs.get("raw"),
        Some(Value::Tensor { tensor }) if tensor.data == TensorData::Bytes(vec![Bytes::from_static(b"plain")])
    ));
}

#[test]
fn decode_output_rejects_bad_payloads() {
    assert!(
        decode_output(TritonOutput {
            name: "bad".to_owned(),
            shape: vec![1],
            datatype: "FP16".to_owned(),
            data: serde_json::json!([1]),
            parameters: None,
        })
        .is_err()
    );
    assert!(numeric_f32(&serde_json::json!({"not": "array"})).is_err());
    assert!(numeric_i64(&serde_json::json!([1.5])).is_err());
    assert!(bytes_data(&serde_json::json!([1]), None).is_err());
    assert!(
        bytes_data(
            &serde_json::json!(["not-base64"]),
            Some(&HashMap::from([(
                "content_encoding".to_owned(),
                serde_json::json!("base64")
            )]))
        )
        .is_err()
    );
}

#[test]
fn authz_denial_returns_authorization_error() {
    let config = Config::default();
    let descriptor = descriptor_from_config(&config);
    let request = PredictRequest {
        model_name: "model".to_owned(),
        ..PredictRequest::default()
    };
    let decider = DenyDecider;
    let err =
        authorize_prediction(Some(&decider), &descriptor, &request).expect_err("denied request");
    assert!(matches!(err, InferenceError::Authorization(_)));
}

#[tokio::test]
async fn configured_decider_is_used_by_registered_adapter() {
    let mut registry = Registry::new();
    let config = Config::default().with_decider(Arc::new(DenyDecider));
    register(&mut registry, config).expect("register triton");
    let adapter = registry.build(TRITON_KIND).expect("build triton");
    let err = adapter
        .predict(PredictRequest {
            model_name: "model".to_owned(),
            ..PredictRequest::default()
        })
        .await
        .expect_err("denied request");
    assert!(matches!(err, InferenceError::Authorization(_)));
}

#[test]
fn register_adds_triton_kind() {
    let mut registry = Registry::new();
    register(&mut registry, Config::default()).expect("register triton");
    assert_eq!(registry.kinds(), vec![TRITON_KIND.to_owned()]);
}

#[test]
fn config_builders_and_debug_redact_runtime_hooks() {
    let config = Config::default()
        .with_policy(Policy::new())
        .with_decider(Arc::new(DenyDecider));

    let debug = format!("{config:?}");

    assert!(debug.contains("<configured>"));
    assert!(config.policy.is_some());
    assert!(config.decider.is_some());
}

#[tokio::test]
async fn component_and_request_response_fast_paths() {
    let adapter = TritonInference::new(Config::default()).unwrap();

    assert_eq!(rskit_provider::Provider::name(&adapter), TRITON_KIND);
    assert_eq!(rskit_component::Component::name(&adapter), "triton");
    rskit_component::Component::start(&adapter).await.unwrap();
    rskit_component::Component::stop(&adapter).await.unwrap();
    assert!(rskit_component::Component::health(&adapter).is_healthy());
    assert_eq!(adapter.descriptor().name, "triton");
}

#[test]
fn merged_wire_parameters_rejects_non_object_options() {
    let request = PredictRequest {
        options: serde_json::json!("bad"),
        ..PredictRequest::default()
    };

    let err = merged_wire_parameters(&request).unwrap_err();

    assert!(matches!(err, InferenceError::Decode(_)));
}

#[test]
fn numeric_and_bytes_helpers_accept_scalar_bytes_and_empty_usage() {
    assert_eq!(
        bytes_data(&serde_json::json!("plain"), None).unwrap(),
        vec![Bytes::from_static(b"plain")]
    );
    assert_eq!(decode_usage(None).input_tokens, 0);
}
