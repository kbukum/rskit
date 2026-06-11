use bytes::Bytes;
use rskit_errors::{AppError, ErrorCode};
use rskit_inference::{
    InferenceError, PredictResponse, PredictStatus, ServingProtocol, Tensor, TensorData, Value,
};

fn assert_round_trip(value: Value) {
    let encoded = serde_json::to_string(&value).expect("serialize value");
    let decoded: Value = serde_json::from_str(&encoded).expect("deserialize value");
    assert_eq!(decoded, value);
}

#[test]
fn value_variants_round_trip() {
    assert_round_trip(Value::Text {
        text: "hello".to_owned(),
    });
    assert_round_trip(Value::Bytes {
        bytes: Bytes::from_static(b"hello"),
    });
    assert_round_trip(Value::Json {
        json: serde_json::json!({"ok": true, "items": [1, 2, 3]}),
    });
    assert_round_trip(Value::Tensor {
        tensor: Tensor {
            dtype: "FP32".to_owned(),
            shape: vec![2],
            data: TensorData::F32(vec![1.0, 2.0]),
        },
    });
}

#[test]
fn tensor_numeric_data_round_trips_by_dtype() {
    let cases = vec![
        Tensor {
            dtype: "FP32".to_owned(),
            shape: vec![2],
            data: TensorData::F32(vec![1.25, 2.5]),
        },
        Tensor {
            dtype: "FP64".to_owned(),
            shape: vec![2],
            data: TensorData::F64(vec![1.25, 2.5]),
        },
        Tensor {
            dtype: "INT32".to_owned(),
            shape: vec![2],
            data: TensorData::I32(vec![1, 2]),
        },
        Tensor {
            dtype: "INT64".to_owned(),
            shape: vec![2],
            data: TensorData::I64(vec![1, 2]),
        },
        Tensor {
            dtype: "UINT8".to_owned(),
            shape: vec![3],
            data: TensorData::U8(vec![1, 2, 3]),
        },
        Tensor {
            dtype: "BOOL".to_owned(),
            shape: vec![2],
            data: TensorData::Bool(vec![true, false]),
        },
        Tensor {
            dtype: "BYTES".to_owned(),
            shape: vec![2],
            data: TensorData::Bytes(vec![Bytes::from_static(b"a"), Bytes::from_static(b"b")]),
        },
    ];

    for tensor in cases {
        assert_round_trip(Value::Tensor { tensor });
    }
}

#[test]
fn tensor_deserialize_normalizes_dtype_for_data_selection_but_preserves_wire_dtype() {
    let tensor: Tensor = serde_json::from_value(serde_json::json!({
        "dtype": "fp32",
        "shape": [2],
        "data": [1.25, 2.5]
    }))
    .expect("deserialize lowercase dtype");

    assert_eq!(tensor.dtype, "fp32");
    assert_eq!(tensor.shape, vec![2]);
    assert_eq!(tensor.data, TensorData::F32(vec![1.25, 2.5]));
}

#[test]
fn tensor_unknown_dtype_uses_serde_untagged_fallback() {
    let tensor: Tensor = serde_json::from_value(serde_json::json!({
        "dtype": "CUSTOM_INT",
        "shape": [3],
        "data": [1, 2, 3]
    }))
    .expect("deserialize unknown dtype");

    assert_eq!(tensor.dtype, "CUSTOM_INT");
    assert_eq!(tensor.data, TensorData::F32(vec![1.0, 2.0, 3.0]));
}

#[test]
fn predict_response_default_is_successful_unknown_model() {
    let response = PredictResponse::default();

    assert!(response.outputs.is_empty());
    assert!(response.metadata.is_empty());
    assert_eq!(response.model.name, "");
    assert_eq!(
        response.model.provider,
        rskit_ai::Provider::Custom("unknown".to_owned())
    );
    assert_eq!(response.status, PredictStatus::Success);
}

#[test]
fn serving_protocol_serializes_all_known_variants() {
    let cases = [
        (ServingProtocol::KServeV2Http, "\"k_serve_v2_http\""),
        (ServingProtocol::KServeV2Grpc, "\"k_serve_v2_grpc\""),
        (ServingProtocol::VllmRest, "\"vllm_rest\""),
        (ServingProtocol::TgiRest, "\"tgi_rest\""),
        (ServingProtocol::BentoMl, "\"bento_ml\""),
        (ServingProtocol::OnnxRuntime, "\"onnx_runtime\""),
        (ServingProtocol::TfServing, "\"tf_serving\""),
        (ServingProtocol::Custom, "\"custom\""),
    ];

    for (protocol, expected) in cases {
        let encoded = serde_json::to_string(&protocol).expect("serialize protocol");
        assert_eq!(encoded, expected);
        let decoded: ServingProtocol =
            serde_json::from_str(&encoded).expect("deserialize protocol");
        assert_eq!(decoded, protocol);
    }
}

#[test]
fn app_error_to_inference_error_preserves_typed_categories() {
    let cases = [
        (
            AppError::timeout("inference runtime"),
            "timeout",
            ErrorCode::Timeout,
        ),
        (
            AppError::cancelled("inference runtime"),
            "cancelled",
            ErrorCode::Cancelled,
        ),
        (
            AppError::connection_failed("runtime"),
            "transport",
            ErrorCode::ExternalService,
        ),
        (
            AppError::new(ErrorCode::InvalidInput, "bad tensor"),
            "invalid input",
            ErrorCode::InvalidInput,
        ),
        (
            AppError::new(ErrorCode::MissingField, "model_name"),
            "invalid input",
            ErrorCode::InvalidInput,
        ),
        (
            AppError::new(ErrorCode::InvalidFormat, "shape"),
            "invalid input",
            ErrorCode::InvalidInput,
        ),
        (
            AppError::new(ErrorCode::Internal, "policy rejected"),
            "policy",
            ErrorCode::ExternalService,
        ),
    ];

    for (app_error, display_fragment, round_trip_code) in cases {
        let inference_error = InferenceError::from(app_error);
        assert!(
            inference_error.to_string().contains(display_fragment),
            "{inference_error}"
        );
        let round_trip: AppError = inference_error.into();
        assert_eq!(round_trip.code(), round_trip_code);
    }
}

#[test]
fn inference_error_to_app_error_maps_public_variants() {
    let cases = [
        (
            InferenceError::Authorization("scope missing".to_owned()),
            ErrorCode::Forbidden,
            "scope missing",
        ),
        (
            InferenceError::InvalidInput("bad request".to_owned()),
            ErrorCode::InvalidInput,
            "bad request",
        ),
        (
            InferenceError::Server {
                status: 503,
                body: "unavailable".to_owned(),
            },
            ErrorCode::ExternalService,
            "status 503",
        ),
        (
            InferenceError::Decode("invalid json".to_owned()),
            ErrorCode::ExternalService,
            "decode",
        ),
        (
            InferenceError::NotImplemented("streaming"),
            ErrorCode::ExternalService,
            "not implemented",
        ),
    ];

    for (inference_error, code, message_fragment) in cases {
        let app_error: AppError = inference_error.into();
        assert_eq!(app_error.code(), code);
        assert!(
            app_error.message().contains(message_fragment),
            "{}",
            app_error.message()
        );
    }
}

#[test]
fn transport_error_conversion_keeps_cause() {
    let source = AppError::connection_failed("runtime");
    let app_error: AppError = InferenceError::Transport(source).into();

    assert_eq!(app_error.code(), ErrorCode::ExternalService);
    assert!(app_error.message().contains("inference transport failed"));
    assert!(app_error.cause().is_some());
}

#[test]
fn serving_protocol_and_error_conversions_round_trip() {
    use std::error::Error;

    let protocol = ServingProtocol::KServeV2Http;
    let encoded = serde_json::to_string(&protocol).expect("serialize protocol");
    assert_eq!(encoded, "\"k_serve_v2_http\"");
    let decoded: ServingProtocol = serde_json::from_str(&encoded).expect("deserialize protocol");
    assert_eq!(decoded, protocol);

    let timeout: InferenceError = AppError::timeout("test").into();
    assert!(matches!(timeout, InferenceError::Timeout));
    let transport: InferenceError = AppError::connection_failed("runtime").into();
    assert!(matches!(
        transport.source().map(ToString::to_string).as_deref(),
        Some(source) if source.contains("connection failed: runtime")
    ));
    let app_error: AppError = InferenceError::Cancelled.into();
    assert_eq!(app_error.code(), ErrorCode::Cancelled);
}
