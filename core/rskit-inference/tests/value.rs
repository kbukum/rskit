use bytes::Bytes;
use rskit_inference::{Tensor, TensorData, Value};

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
fn serving_protocol_and_error_conversions_round_trip() {
    let protocol = rskit_inference::ServingProtocol::KServeV2Http;
    let encoded = serde_json::to_string(&protocol).expect("serialize protocol");
    assert_eq!(encoded, "\"k_serve_v2_http\"");
    let decoded: rskit_inference::ServingProtocol =
        serde_json::from_str(&encoded).expect("deserialize protocol");
    assert_eq!(decoded, protocol);

    let timeout: rskit_inference::InferenceError = rskit_errors::AppError::timeout("test").into();
    assert!(matches!(timeout, rskit_inference::InferenceError::Timeout));
    let app_error: rskit_errors::AppError = rskit_inference::InferenceError::Cancelled.into();
    assert_eq!(app_error.code, rskit_errors::ErrorCode::Cancelled);
}
