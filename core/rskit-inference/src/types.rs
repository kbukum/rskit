use std::collections::HashMap;

use bytes::Bytes;
pub use rskit_ai::{
    Capabilities, Model, Provider as ModelProvider, StreamEvent, StreamEventRef, Usage,
};
use rskit_errors::{AppError, ErrorCode};
use serde::{Deserialize, Serialize};
use thiserror::Error;

/// Runtime-neutral model prediction request.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct PredictRequest {
    /// Optional request id. Adapters generate UUID v7 when absent.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub request_id: Option<String>,
    /// Runtime model name.
    pub model_name: String,
    /// Optional runtime model version.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub model_version: Option<String>,
    /// Named inputs consumed by the serving runtime.
    #[serde(default)]
    pub inputs: HashMap<String, Value>,
    /// Adapter-specific knobs such as `max_new_tokens`, `temperature`, or `top_k`.
    #[serde(default)]
    pub parameters: HashMap<String, serde_json::Value>,
    /// Provider-specific runtime options.
    #[serde(default)]
    pub options: serde_json::Value,
    /// Request metadata for authz, tracing, and adapter-specific headers.
    #[serde(default)]
    pub metadata: HashMap<String, String>,
}

/// Runtime-neutral prediction response.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PredictResponse {
    /// Named outputs returned by the serving runtime.
    #[serde(default)]
    pub outputs: HashMap<String, Value>,
    /// Token and compute usage counters.
    #[serde(default)]
    pub usage: Usage,
    /// Model that served the request.
    pub model: Model,
    /// Prediction status.
    pub status: PredictStatus,
    /// Response metadata such as model version or finish reason.
    #[serde(default)]
    pub metadata: HashMap<String, String>,
}

impl Default for PredictResponse {
    fn default() -> Self {
        Self {
            outputs: HashMap::new(),
            usage: Usage::default(),
            model: Model {
                name: String::new(),
                provider: ModelProvider::Custom("unknown".to_string()),
                version: None,
                capabilities: Capabilities::default(),
            },
            status: PredictStatus::Success,
            metadata: HashMap::new(),
        }
    }
}

/// Prediction status.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
#[non_exhaustive]
pub enum PredictStatus {
    /// Prediction completed successfully.
    Success,
    /// Prediction completed with partial outputs.
    PartialSuccess,
    /// Serving runtime reported an error.
    Error {
        /// Error reason.
        reason: String,
    },
}

/// Runtime-neutral input or output value.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum Value {
    /// UTF-8 text value.
    Text {
        /// Text content.
        text: String,
    },
    /// Opaque bytes value.
    Bytes {
        /// Byte content.
        bytes: Bytes,
    },
    /// KServe v2-style tensor value.
    Tensor {
        /// Tensor content.
        tensor: Tensor,
    },
    /// Arbitrary JSON value for runtimes that accept structured values directly.
    Json {
        /// JSON content.
        json: serde_json::Value,
    },
}

/// KServe v2-style tensor value.
#[derive(Debug, Clone, PartialEq)]
pub struct Tensor {
    /// KServe v2 dtype name: `FP32`, `FP64`, `INT32`, `INT64`, `UINT8`, `BOOL`, `BYTES`, ...
    pub dtype: String,
    /// Tensor shape.
    pub shape: Vec<i64>,
    /// Tensor data.
    pub data: TensorData,
}

impl Serialize for Tensor {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        use serde::ser::SerializeStruct;

        let mut state = serializer.serialize_struct("Tensor", 3)?;
        state.serialize_field("dtype", &self.dtype)?;
        state.serialize_field("shape", &self.shape)?;
        state.serialize_field("data", &self.data)?;
        state.end()
    }
}

impl<'de> Deserialize<'de> for Tensor {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        #[derive(Deserialize)]
        struct TensorWire {
            dtype: String,
            shape: Vec<i64>,
            data: serde_json::Value,
        }

        let wire = TensorWire::deserialize(deserializer)?;
        let dtype = wire.dtype.to_ascii_uppercase();
        let data = match dtype.as_str() {
            "FP32" => TensorData::F32(
                serde_json::from_value(wire.data).map_err(serde::de::Error::custom)?,
            ),
            "FP64" => TensorData::F64(
                serde_json::from_value(wire.data).map_err(serde::de::Error::custom)?,
            ),
            "INT32" => TensorData::I32(
                serde_json::from_value(wire.data).map_err(serde::de::Error::custom)?,
            ),
            "INT64" => TensorData::I64(
                serde_json::from_value(wire.data).map_err(serde::de::Error::custom)?,
            ),
            "UINT8" => {
                TensorData::U8(serde_json::from_value(wire.data).map_err(serde::de::Error::custom)?)
            }
            "BOOL" => TensorData::Bool(
                serde_json::from_value(wire.data).map_err(serde::de::Error::custom)?,
            ),
            "BYTES" => TensorData::Bytes(
                serde_json::from_value(wire.data).map_err(serde::de::Error::custom)?,
            ),
            _ => serde_json::from_value(wire.data).map_err(serde::de::Error::custom)?,
        };
        Ok(Self {
            dtype: wire.dtype,
            shape: wire.shape,
            data,
        })
    }
}

/// Tensor data variants supported by the core contract.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum TensorData {
    /// 32-bit floating-point values.
    F32(Vec<f32>),
    /// 64-bit floating-point values.
    F64(Vec<f64>),
    /// 32-bit signed integer values.
    I32(Vec<i32>),
    /// 64-bit signed integer values.
    I64(Vec<i64>),
    /// 8-bit unsigned integer values.
    U8(Vec<u8>),
    /// Boolean values.
    Bool(Vec<bool>),
    /// Bytes values.
    Bytes(Vec<Bytes>),
}

/// Adapter capability and executable authority declaration.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct InferenceDescriptor {
    /// Stable adapter name.
    pub name: String,
    /// Human-readable adapter description.
    pub description: String,
    /// Serving protocol implemented by this adapter.
    pub serving_protocol: ServingProtocol,
    /// Executable permission envelope declared by the adapter.
    pub envelope: rskit_tool::Envelope,
}

/// Model-serving runtime protocol.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
#[non_exhaustive]
pub enum ServingProtocol {
    /// KServe v2 over HTTP.
    KServeV2Http,
    /// KServe v2 over gRPC.
    KServeV2Grpc,
    /// vLLM raw REST generation APIs.
    VllmRest,
    /// Hugging Face Text Generation Inference REST APIs.
    TgiRest,
    /// BentoML serving APIs.
    BentoMl,
    /// ONNX Runtime serving APIs.
    OnnxRuntime,
    /// TensorFlow Serving APIs.
    TfServing,
    /// Custom runtime protocol.
    Custom,
}

/// Typed inference failure.
#[derive(Debug, Error)]
#[non_exhaustive]
pub enum InferenceError {
    /// Transport error from the injected HTTP client.
    #[error("transport: {0}")]
    Transport(String),
    /// Response decode or protocol mapping failure.
    #[error("decode: {0}")]
    Decode(String),
    /// Serving runtime returned an unsuccessful response.
    #[error("server: status={status}, body={body}")]
    Server {
        /// HTTP or protocol status code.
        status: u16,
        /// Runtime response body.
        body: String,
    },
    /// Request was denied by an injected authorization decider.
    #[error("authorization denied: {0}")]
    Authorization(String),
    /// Injected resilience policy failed the operation.
    #[error("policy: {0}")]
    Policy(String),
    /// Requested adapter capability is not implemented yet.
    #[error("not implemented: {0}")]
    NotImplemented(&'static str),
    /// Operation was cancelled.
    #[error("cancelled")]
    Cancelled,
    /// Operation timed out.
    #[error("timeout")]
    Timeout,
}

impl From<AppError> for InferenceError {
    fn from(value: AppError) -> Self {
        match value.code {
            ErrorCode::Timeout => Self::Timeout,
            ErrorCode::Cancelled => Self::Cancelled,
            ErrorCode::ExternalService
            | ErrorCode::ServiceUnavailable
            | ErrorCode::ConnectionFailed => Self::Transport(value.to_string()),
            _ => Self::Policy(value.to_string()),
        }
    }
}

impl From<InferenceError> for AppError {
    fn from(value: InferenceError) -> Self {
        match value {
            InferenceError::Timeout => AppError::timeout("inference"),
            InferenceError::Cancelled => AppError::cancelled("inference"),
            InferenceError::Authorization(reason) => AppError::forbidden(reason),
            InferenceError::Server { status, body } => AppError::new(
                ErrorCode::ExternalService,
                format!("inference runtime returned status {status}: {body}"),
            ),
            other => AppError::new(ErrorCode::ExternalService, other.to_string()),
        }
    }
}
