//! Triton KServe v2 HTTP adapter for `rskit-inference`.
//!
//! This crate implements non-streaming [`rskit_inference::Inference`] against
//! Triton's KServe v2 HTTP data plane. KServe v2 HTTP has no native streaming
//! protocol, so this adapter intentionally does not implement
//! [`rskit_inference::StreamingInference`].

#![warn(missing_docs)]

use std::{collections::HashMap, sync::Arc};

use async_trait::async_trait;
use base64::{Engine, engine::general_purpose::STANDARD as BASE64_STANDARD};
use bytes::Bytes;
use rskit_ai::{Capabilities, Model, Provider as ModelProvider, semconv, semconv::Operation};
use rskit_authz::{AuthzDecision, AuthzRequest, Decider};
use rskit_errors::AppResult;
use rskit_httpclient::{HttpClient, HttpClientConfig, Request};
use rskit_inference::{
    Factory, Inference, InferenceDescriptor, InferenceError, PredictRequest, PredictResponse,
    PredictStatus, Registry, RegistryError, ServingProtocol, Tensor, TensorData, Usage, Value,
};
use rskit_resilience::Policy;
use rskit_tool::{Envelope, NetworkPolicy, NetworkRule};
use serde::{Deserialize, Serialize};
use serde_json::json;
use tracing::Instrument;

/// Registry kind for the Triton KServe v2 HTTP adapter.
pub const TRITON_KIND: &str = "triton";

const SYSTEM: &str = "triton";

/// Configuration for Triton KServe v2 HTTP serving.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TritonConfig {
    /// Base URL for the Triton HTTP endpoint.
    #[serde(default = "default_base_url")]
    pub base_url: String,
    /// Descriptor name.
    #[serde(default = "default_name")]
    pub name: String,
    /// Descriptor description.
    #[serde(default = "default_description")]
    pub description: String,
    /// Network host declared in the executable envelope.
    #[serde(default = "default_network_host")]
    pub network_host: String,
    /// Network port declared in the executable envelope.
    #[serde(default = "default_network_port")]
    pub network_port: Option<u16>,
    /// Network scheme declared in the executable envelope.
    #[serde(default = "default_network_scheme")]
    pub network_scheme: String,
    /// Authz scopes declared in the executable envelope.
    #[serde(default = "default_scopes")]
    pub scopes: Vec<String>,
}

impl Default for TritonConfig {
    fn default() -> Self {
        Self {
            base_url: default_base_url(),
            name: default_name(),
            description: default_description(),
            network_host: default_network_host(),
            network_port: default_network_port(),
            network_scheme: default_network_scheme(),
            scopes: default_scopes(),
        }
    }
}

/// Triton KServe v2 HTTP inference adapter.
pub struct TritonInference {
    client: HttpClient,
    #[expect(dead_code, reason = "stored for future introspection; original design")]
    config: TritonConfig,
    descriptor: InferenceDescriptor,
    policy: Option<Policy>,
    decider: Option<Arc<dyn Decider>>,
}

#[async_trait]
impl rskit_provider::Provider for TritonInference {
    fn name(&self) -> &'static str {
        TRITON_KIND
    }
}

#[async_trait]
impl rskit_provider::RequestResponse<PredictRequest, PredictResponse> for TritonInference {
    async fn execute(&self, input: PredictRequest) -> AppResult<PredictResponse> {
        self.predict(input).await.map_err(Into::into)
    }
}

impl TritonInference {
    /// Create a Triton adapter with the default canonical HTTP client.
    pub fn new(config: TritonConfig) -> Result<Self, InferenceError> {
        let client = HttpClient::new(
            HttpClientConfig::new().with_base_url(config.base_url.trim_end_matches('/')),
        )?;
        Ok(Self::with_http_client(config, client))
    }

    /// Create a Triton adapter with an injected canonical HTTP client.
    #[must_use]
    pub fn with_http_client(config: TritonConfig, client: HttpClient) -> Self {
        let descriptor = descriptor_from_config(&config);
        Self {
            client,
            config,
            descriptor,
            policy: None,
            decider: None,
        }
    }

    /// Create a Triton adapter with an injected raw reqwest client.
    #[must_use]
    pub fn with_reqwest_client(config: TritonConfig, client: reqwest::Client) -> Self {
        let http_client = HttpClient::from_parts(
            HttpClientConfig::new().with_base_url(config.base_url.trim_end_matches('/')),
            client,
        );
        Self::with_http_client(config, http_client)
    }

    /// Inject a resilience policy. The adapter does not implement inline retries.
    #[must_use]
    pub fn with_policy(mut self, policy: Policy) -> Self {
        self.policy = Some(policy);
        self
    }

    /// Inject an authorization decider. Absence means open-by-default.
    #[must_use]
    pub fn with_decider(mut self, decider: Arc<dyn Decider>) -> Self {
        self.decider = Some(decider);
        self
    }

    /// Return whether Triton reports `/v2/health/ready` successfully.
    pub async fn health_check(&self) -> Result<bool, InferenceError> {
        Ok(self.client.get("/v2/health/ready").await?.is_success())
    }

    async fn predict_authorized(
        &self,
        request: PredictRequest,
    ) -> Result<PredictResponse, InferenceError> {
        let mut request = request;
        let request_id = request
            .request_id
            .get_or_insert_with(|| uuid::Uuid::now_v7().to_string())
            .clone();
        authorize_prediction(self.decider.as_deref(), &self.descriptor, &request)?;
        let operation = operation_name(&request);
        let model_version = request.model_version.clone().unwrap_or_default();
        let span = tracing::info_span!(
            "inference.predict",
            gen_ai.system = SYSTEM,
            gen_ai.operation.name = operation.as_str(),
            gen_ai.request.model = request.model_name.as_str(),
            gen_ai.request.model.version = model_version.as_str(),
            gen_ai.request.id = request_id.as_str(),
            gen_ai.usage.input_tokens = tracing::field::Empty,
            gen_ai.usage.output_tokens = tracing::field::Empty,
            gen_ai.usage.cached_tokens = tracing::field::Empty,
            gen_ai.response.model = tracing::field::Empty,
            gen_ai.response.finish_reason = tracing::field::Empty,
        );

        async move {
            let response = self.predict_once(&request).await?;
            let current = tracing::Span::current();
            current.record(semconv::USAGE_INPUT_TOKENS, response.usage.input_tokens);
            current.record(semconv::USAGE_OUTPUT_TOKENS, response.usage.output_tokens);
            current.record(semconv::USAGE_CACHED_TOKENS, response.usage.cached_tokens);
            current.record(semconv::RESPONSE_MODEL, response.model.name.as_str());
            if let Some(reason) = response.metadata.get("finish_reason") {
                current.record(semconv::RESPONSE_FINISH_REASON, reason.as_str());
            }
            Ok(response)
        }
        .instrument(span)
        .await
    }

    async fn predict_once(
        &self,
        request: &PredictRequest,
    ) -> Result<PredictResponse, InferenceError> {
        let path = infer_path(&request.model_name, request.model_version.as_deref())?;
        let response = self
            .client
            .send(
                Request::post(path)
                    .json_body(&encode_request(request)?)
                    .map_err(|err| InferenceError::Decode(err.to_string()))?,
            )
            .await?;
        let status = response.status();
        let body = response
            .text()
            .map_err(|err| InferenceError::Decode(err.to_string()))?;
        if !status.is_success() {
            return Err(InferenceError::Server {
                status: status.as_u16(),
                body,
            });
        }
        let decoded = serde_json::from_str::<TritonResponse>(&body)
            .map_err(|err| InferenceError::Decode(err.to_string()))?;
        decode_response(decoded)
    }
}

#[async_trait]
impl Inference for TritonInference {
    async fn predict(&self, request: PredictRequest) -> Result<PredictResponse, InferenceError> {
        if let Some(policy) = &self.policy {
            policy
                .execute(|| self.predict_authorized(request.clone()))
                .await
        } else {
            self.predict_authorized(request).await
        }
    }

    fn descriptor(&self) -> InferenceDescriptor {
        self.descriptor.clone()
    }
}

#[async_trait]
impl rskit_component::Component for TritonInference {
    fn name(&self) -> &str {
        &self.descriptor.name
    }

    async fn start(&self) -> rskit_errors::AppResult<()> {
        Ok(())
    }

    async fn stop(&self) -> rskit_errors::AppResult<()> {
        Ok(())
    }

    fn health(&self) -> rskit_component::Health {
        rskit_component::Health::healthy(self.name())
    }
}

/// Register the Triton adapter factory in an explicit registry.
pub fn register(registry: &mut Registry) -> Result<(), RegistryError> {
    let factory: Factory = Arc::new(|config| {
        let config = if config.is_null() {
            TritonConfig::default()
        } else {
            serde_json::from_value::<TritonConfig>(config)
                .map_err(|err| InferenceError::Decode(err.to_string()))?
        };
        Ok(Arc::new(TritonInference::new(config)?))
    });
    registry.register(TRITON_KIND, factory)
}

fn descriptor_from_config(config: &TritonConfig) -> InferenceDescriptor {
    InferenceDescriptor {
        name: config.name.clone(),
        description: config.description.clone(),
        serving_protocol: ServingProtocol::KServeV2Http,
        envelope: Envelope {
            scopes: config.scopes.clone(),
            network: NetworkPolicy::AllowList {
                rules: vec![NetworkRule {
                    host: config.network_host.clone(),
                    port: config.network_port,
                    scheme: Some(config.network_scheme.clone()),
                }],
            },
            ..Envelope::default()
        },
    }
}

fn authorize_prediction(
    decider: Option<&dyn Decider>,
    descriptor: &InferenceDescriptor,
    request: &PredictRequest,
) -> Result<(), InferenceError> {
    let Some(decider) = decider else {
        return Ok(());
    };
    let principal = request
        .metadata
        .get("principal")
        .cloned()
        .unwrap_or_else(|| "anonymous".to_owned());
    let decision = decider.decide(&AuthzRequest {
        principal,
        action: "inference:predict".to_owned(),
        resource: format!("inference:{}:{}", descriptor.name, request.model_name),
        scopes: descriptor.envelope.scopes.clone(),
        attributes: json!({
            "model_name": request.model_name,
            "model_version": request.model_version,
            "serving_protocol": descriptor.serving_protocol,
        }),
    });
    match decision {
        AuthzDecision::Allow => Ok(()),
        AuthzDecision::Deny(reason) | AuthzDecision::RequiresHumanApproval(reason) => {
            Err(InferenceError::Authorization(reason))
        }
        _ => Err(InferenceError::Authorization(
            "unsupported decision".to_owned(),
        )),
    }
}

fn infer_path(model_name: &str, model_version: Option<&str>) -> Result<String, InferenceError> {
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

fn path_segment(value: &str) -> Result<&str, InferenceError> {
    if value.is_empty() || value.contains('/') {
        Err(InferenceError::Decode(
            "model path segments must be non-empty and cannot contain '/'".to_owned(),
        ))
    } else {
        Ok(value)
    }
}

fn operation_name(request: &PredictRequest) -> Operation {
    request
        .parameters
        .get("operation")
        .and_then(serde_json::Value::as_str)
        .and_then(Operation::from_operation_name)
        .unwrap_or(Operation::InferenceRequest)
}

fn encode_request(request: &PredictRequest) -> Result<serde_json::Value, InferenceError> {
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

fn merged_wire_parameters(
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

fn encode_input(name: &str, value: &Value) -> Result<serde_json::Value, InferenceError> {
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

fn encode_tensor(name: &str, tensor: &Tensor) -> Result<serde_json::Value, InferenceError> {
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

fn decode_response(response: TritonResponse) -> Result<PredictResponse, InferenceError> {
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

fn decode_output(output: TritonOutput) -> Result<(String, Value), InferenceError> {
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

fn numeric_f32(value: &serde_json::Value) -> Result<Vec<f32>, InferenceError> {
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

fn numeric_i64(value: &serde_json::Value) -> Result<Vec<i64>, InferenceError> {
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

fn bytes_data(
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

fn decode_usage(parameters: Option<&HashMap<String, serde_json::Value>>) -> Usage {
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

fn int_parameter(parameters: &HashMap<String, serde_json::Value>, key: &str) -> u64 {
    parameters
        .get(key)
        .and_then(serde_json::Value::as_u64)
        .unwrap_or(0)
}

#[derive(Debug, Deserialize)]
struct TritonResponse {
    #[serde(default)]
    outputs: Vec<TritonOutput>,
    #[serde(default)]
    parameters: Option<HashMap<String, serde_json::Value>>,
    #[serde(default)]
    model_name: Option<String>,
    #[serde(default)]
    model_version: Option<String>,
}

#[derive(Debug, Deserialize)]
struct TritonOutput {
    name: String,
    #[serde(default)]
    shape: Vec<i64>,
    #[serde(alias = "dtype", alias = "datatype")]
    datatype: String,
    #[serde(default)]
    data: serde_json::Value,
    #[serde(default)]
    parameters: Option<HashMap<String, serde_json::Value>>,
}

fn default_base_url() -> String {
    "http://localhost:8000".to_owned()
}

fn default_name() -> String {
    "triton".to_owned()
}

fn default_description() -> String {
    "Triton KServe v2 model-serving adapter".to_owned()
}

fn default_network_host() -> String {
    "localhost".to_owned()
}

fn default_network_port() -> Option<u16> {
    Some(8000)
}

fn default_network_scheme() -> String {
    "http".to_owned()
}

fn default_scopes() -> Vec<String> {
    vec!["inference:predict".to_owned()]
}

#[cfg(test)]
mod tests {
    use super::*;

    struct DenyDecider;

    impl Decider for DenyDecider {
        fn decide(&self, _request: &AuthzRequest) -> AuthzDecision {
            AuthzDecision::Deny("no".to_owned())
        }
    }

    #[test]
    fn config_defaults_and_descriptor_are_declared() {
        let config = TritonConfig::default();
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
        let config = TritonConfig::default();
        let descriptor = descriptor_from_config(&config);
        let request = PredictRequest {
            model_name: "model".to_owned(),
            ..PredictRequest::default()
        };
        let decider = DenyDecider;
        let err = authorize_prediction(Some(&decider), &descriptor, &request)
            .expect_err("denied request");
        assert!(matches!(err, InferenceError::Authorization(_)));
    }

    #[test]
    fn register_adds_triton_kind() {
        let mut registry = Registry::new();
        register(&mut registry).expect("register triton");
        assert_eq!(registry.kinds(), vec![TRITON_KIND.to_owned()]);
    }
}
