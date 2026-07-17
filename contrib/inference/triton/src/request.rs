use base64::{Engine, engine::general_purpose::STANDARD as BASE64_STANDARD};
use rskit_ai::semconv::Operation;
use rskit_inference::{InferenceError, PredictRequest, Tensor, TensorData, Value};
use serde_json::json;

pub(crate) fn infer_path(
    model_name: &str,
    model_version: Option<&str>,
) -> Result<String, InferenceError> {
    let model = path_segment(model_name)?;
    if let Some(version) = model_version {
        Ok(format!(
            "/v2/models/{model}/versions/{}/infer",
            path_segment(version)?
        ))
    } else {
        Ok(format!("/v2/models/{model}/infer"))
    }
}

pub(crate) fn path_segment(value: &str) -> Result<&str, InferenceError> {
    if value.is_empty() || value.contains('/') {
        Err(InferenceError::Decode(
            "model path segments must be non-empty and cannot contain '/'".to_owned(),
        ))
    } else {
        Ok(value)
    }
}

pub(crate) fn operation_name(request: &PredictRequest) -> Operation {
    request
        .parameters
        .get("operation")
        .and_then(serde_json::Value::as_str)
        .and_then(Operation::from_operation_name)
        .unwrap_or(Operation::InferenceRequest)
}

pub(crate) fn encode_request(
    request: &PredictRequest,
) -> Result<serde_json::Value, InferenceError> {
    let mut payload = serde_json::Map::new();
    let inputs = request
        .inputs
        .iter()
        .map(|(name, value)| encode_input(name, value))
        .collect::<Result<Vec<_>, _>>()?;
    payload.insert("inputs".to_owned(), serde_json::Value::Array(inputs));
    if let Some(id) = &request.request_id {
        payload.insert("id".to_owned(), serde_json::Value::String(id.clone()));
    }
    let parameters = merged_wire_parameters(request)?;
    if !parameters.is_empty() {
        payload.insert(
            "parameters".to_owned(),
            serde_json::Value::Object(parameters),
        );
    }
    Ok(serde_json::Value::Object(payload))
}

pub(crate) fn merged_wire_parameters(
    request: &PredictRequest,
) -> Result<serde_json::Map<String, serde_json::Value>, InferenceError> {
    let mut parameters = match &request.options {
        serde_json::Value::Null => serde_json::Map::new(),
        serde_json::Value::Object(options) => options.clone(),
        _ => {
            return Err(InferenceError::Decode(
                "Triton request options must be a JSON object".to_owned(),
            ));
        }
    };
    parameters.extend(request.parameters.clone());
    Ok(parameters)
}

pub(crate) fn encode_input(name: &str, value: &Value) -> Result<serde_json::Value, InferenceError> {
    match value {
        Value::Text { text } => Ok(json!({
            "name": name,
            "shape": [1],
            "datatype": "BYTES",
            "data": [text],
        })),
        Value::Bytes { bytes } => Ok(json!({
            "name": name,
            "shape": [1],
            "datatype": "BYTES",
            "data": [BASE64_STANDARD.encode(bytes)],
            "parameters": {"content_encoding": "base64"},
        })),
        Value::Tensor { tensor } => encode_tensor(name, tensor),
        Value::Json { json: value } => Ok(json!({
            "name": name,
            "shape": [1],
            "datatype": "BYTES",
            "data": [serde_json::to_string(value).map_err(|err| InferenceError::Decode(err.to_string()))?],
            "parameters": {"content_type": "application/json"},
        })),
    }
}

pub(crate) fn encode_tensor(
    name: &str,
    tensor: &Tensor,
) -> Result<serde_json::Value, InferenceError> {
    let dtype = tensor.dtype.to_ascii_uppercase();
    match (&dtype[..], &tensor.data) {
        ("FP32", TensorData::F32(values)) => Ok(json!({
            "name": name,
            "shape": tensor.shape,
            "datatype": dtype,
            "data": values,
        })),
        ("INT64", TensorData::I64(values)) => Ok(json!({
            "name": name,
            "shape": tensor.shape,
            "datatype": dtype,
            "data": values,
        })),
        ("BYTES", TensorData::Bytes(values)) => {
            let encoded = values
                .iter()
                .map(|value| BASE64_STANDARD.encode(value))
                .collect::<Vec<_>>();
            Ok(json!({
                "name": name,
                "shape": tensor.shape,
                "datatype": dtype,
                "data": encoded,
                "parameters": {"content_encoding": "base64"},
            }))
        }
        _ => Err(InferenceError::Decode(format!(
            "unsupported Triton tensor dtype {:?}",
            tensor.dtype
        ))),
    }
}
