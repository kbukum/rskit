use std::collections::HashMap;

use base64::{Engine, engine::general_purpose::STANDARD as BASE64_STANDARD};
use bytes::Bytes;
use rskit_ai::{Capabilities, Model, Provider as ModelProvider, Usage};
use rskit_inference::{InferenceError, PredictResponse, PredictStatus, Tensor, TensorData, Value};
use serde::Deserialize;

pub(crate) fn decode_response(response: TritonResponse) -> Result<PredictResponse, InferenceError> {
    let outputs = response
        .outputs
        .into_iter()
        .map(decode_output)
        .collect::<Result<HashMap<_, _>, _>>()?;
    let mut metadata = HashMap::new();
    let model_name = response.model_name.unwrap_or_default();
    let model_version = response.model_version;
    if !model_name.is_empty() {
        metadata.insert("model_name".to_owned(), model_name.clone());
    }
    if let Some(version) = &model_version {
        metadata.insert("model_version".to_owned(), version.clone());
    }
    let usage = decode_usage(response.parameters.as_ref());
    Ok(PredictResponse {
        outputs,
        usage,
        model: Model {
            name: model_name,
            provider: ModelProvider::Triton,
            version: model_version,
            capabilities: Capabilities::default(),
        },
        status: PredictStatus::Success,
        metadata,
    })
}

pub(crate) fn decode_output(output: TritonOutput) -> Result<(String, Value), InferenceError> {
    let dtype = output.datatype.to_ascii_uppercase();
    let tensor_data = match dtype.as_str() {
        "FP32" => TensorData::F32(numeric_f32(&output.data)?),
        "INT64" => TensorData::I64(numeric_i64(&output.data)?),
        "BYTES" => TensorData::Bytes(bytes_data(&output.data, output.parameters.as_ref())?),
        _ => {
            return Err(InferenceError::Decode(format!(
                "unsupported Triton response dtype {dtype:?}"
            )));
        }
    };
    Ok((
        output.name,
        Value::Tensor {
            tensor: Tensor {
                dtype,
                shape: output.shape,
                data: tensor_data,
            },
        },
    ))
}

pub(crate) fn numeric_f32(value: &serde_json::Value) -> Result<Vec<f32>, InferenceError> {
    let values = value
        .as_array()
        .ok_or_else(|| InferenceError::Decode("numeric tensor data must be an array".to_owned()))?;
    values
        .iter()
        .map(|item| {
            item.as_f64().map(|number| number as f32).ok_or_else(|| {
                InferenceError::Decode("numeric tensor data contains a non-number".to_owned())
            })
        })
        .collect()
}

pub(crate) fn numeric_i64(value: &serde_json::Value) -> Result<Vec<i64>, InferenceError> {
    let values = value
        .as_array()
        .ok_or_else(|| InferenceError::Decode("numeric tensor data must be an array".to_owned()))?;
    values
        .iter()
        .map(|item| {
            item.as_i64().ok_or_else(|| {
                InferenceError::Decode("integer tensor data contains a non-integer".to_owned())
            })
        })
        .collect()
}

pub(crate) fn bytes_data(
    value: &serde_json::Value,
    parameters: Option<&HashMap<String, serde_json::Value>>,
) -> Result<Vec<Bytes>, InferenceError> {
    let strings = if let Some(text) = value.as_str() {
        vec![text]
    } else {
        value
            .as_array()
            .ok_or_else(|| {
                InferenceError::Decode("BYTES tensor data must be string or array".to_owned())
            })?
            .iter()
            .map(|item| {
                item.as_str().ok_or_else(|| {
                    InferenceError::Decode("BYTES tensor array contains a non-string".to_owned())
                })
            })
            .collect::<Result<Vec<_>, _>>()?
    };
    let base64_encoded = parameters
        .and_then(|params| params.get("content_encoding"))
        .and_then(serde_json::Value::as_str)
        .is_some_and(|encoding| encoding == "base64");
    strings
        .into_iter()
        .map(|item| {
            if base64_encoded {
                BASE64_STANDARD
                    .decode(item)
                    .map(Bytes::from)
                    .map_err(|err| InferenceError::Decode(err.to_string()))
            } else {
                Ok(Bytes::from(item.to_owned()))
            }
        })
        .collect()
}

pub(crate) fn decode_usage(parameters: Option<&HashMap<String, serde_json::Value>>) -> Usage {
    let Some(parameters) = parameters else {
        return Usage::default();
    };
    Usage {
        input_tokens: int_parameter(parameters, "input_tokens"),
        output_tokens: int_parameter(parameters, "output_tokens"),
        cached_tokens: int_parameter(parameters, "cached_tokens"),
        reasoning_tokens: int_parameter(parameters, "reasoning_tokens"),
    }
}

pub(crate) fn int_parameter(parameters: &HashMap<String, serde_json::Value>, key: &str) -> u64 {
    parameters
        .get(key)
        .and_then(serde_json::Value::as_u64)
        .unwrap_or(0)
}

#[derive(Debug, Deserialize)]
pub(crate) struct TritonResponse {
    #[serde(default)]
    pub(crate) outputs: Vec<TritonOutput>,
    #[serde(default)]
    pub(crate) parameters: Option<HashMap<String, serde_json::Value>>,
    #[serde(default)]
    pub(crate) model_name: Option<String>,
    #[serde(default)]
    pub(crate) model_version: Option<String>,
}

#[derive(Debug, Deserialize)]
pub(crate) struct TritonOutput {
    pub(crate) name: String,
    #[serde(default)]
    pub(crate) shape: Vec<i64>,
    #[serde(alias = "dtype", alias = "datatype")]
    pub(crate) datatype: String,
    #[serde(default)]
    pub(crate) data: serde_json::Value,
    #[serde(default)]
    pub(crate) parameters: Option<HashMap<String, serde_json::Value>>,
}
