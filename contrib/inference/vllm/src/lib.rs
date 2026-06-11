//! vLLM inference adapter using the OAI-compatible `/v1/completions` endpoint.
//!
//! Optional bearer credentials are configured through [`Config::api_key`] as a
//! redacting [`SecretString`] and installed through `rskit-httpclient` auth
//! rather than raw headers.

#![warn(missing_docs)]

use std::sync::Arc;

use async_trait::async_trait;
use rskit_ai::{Capabilities, Model, Provider as ModelProvider, StreamEventRef, Usage};
use rskit_component::{Component, Health};
use rskit_errors::AppResult;
use rskit_httpclient::{Auth, HttpClient, HttpClientConfig, Request};
use rskit_inference::{
    Factory, Inference, InferenceDescriptor, InferenceError, PredictRequest, PredictResponse,
    PredictStatus, Registry, RegistryError, ServingProtocol, StreamingInference, Value,
};
use rskit_tool::Envelope;
use rskit_util::SecretString;
use serde::{Deserialize, Serialize};
use tokio_stream::Stream;

const VLLM_KIND: &str = "vllm";

/// Configuration for the vLLM OAI-compatible adapter.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Config {
    /// Base URL of the vLLM server, for example `http://localhost:8000`.
    pub base_url: String,
    /// Default model name if not provided in the request.
    #[serde(default = "default_model")]
    pub model: String,
    /// Optional bearer token for authenticated vLLM deployments.
    ///
    /// The value is redacted in debug output and serialization.
    #[serde(default)]
    pub api_key: Option<SecretString>,
    /// Max tokens for generation.
    #[serde(default = "default_max_tokens")]
    pub max_tokens: u32,
}

fn default_model() -> String {
    "default".into()
}

fn default_max_tokens() -> u32 {
    256
}

/// vLLM adapter using the OAI-compatible text generation endpoint.
pub(crate) struct VllmAdapter {
    client: HttpClient,
    config: Config,
}

impl VllmAdapter {
    /// Create a new vLLM adapter from config.
    pub(crate) fn new(config: Config) -> AppResult<Self> {
        let mut http_config = HttpClientConfig::new().with_base_url(&config.base_url);
        if let Some(key) = &config.api_key {
            http_config = http_config.with_auth(Auth::bearer_secret(key.clone()));
        }
        Ok(Self {
            client: HttpClient::new(http_config)?,
            config,
        })
    }
}

fn vllm_completion_body(config: &Config, request: &PredictRequest) -> OaiCompletionRequest {
    let prompt = request
        .inputs
        .get("prompt")
        .or_else(|| request.inputs.get("text"))
        .and_then(|value| match value {
            Value::Text { text } => Some(text.clone()),
            _ => None,
        })
        .unwrap_or_default();

    let model = if request.model_name.is_empty() {
        config.model.clone()
    } else {
        request.model_name.clone()
    };

    let max_tokens = request
        .parameters
        .get("max_tokens")
        .and_then(serde_json::Value::as_u64)
        .map(|value| value as u32)
        .unwrap_or(config.max_tokens);

    let temperature = request
        .parameters
        .get("temperature")
        .and_then(serde_json::Value::as_f64)
        .map(|value| value as f32);

    OaiCompletionRequest {
        model,
        prompt,
        max_tokens,
        temperature,
        stream: false,
    }
}

fn vllm_predict_response(
    oai: OaiCompletionResponse,
    model_version: Option<String>,
) -> PredictResponse {
    let generated = oai
        .choices
        .first()
        .map(|choice| choice.text.clone())
        .unwrap_or_default();
    let finish = oai
        .choices
        .first()
        .and_then(|choice| choice.finish_reason.as_deref())
        .map(|reason| ("finish_reason".to_string(), reason.to_string()))
        .into_iter()
        .collect();

    PredictResponse {
        outputs: std::collections::HashMap::from([(
            "text".to_string(),
            Value::Text { text: generated },
        )]),
        usage: Usage {
            input_tokens: oai.usage.prompt_tokens as u64,
            output_tokens: oai.usage.completion_tokens as u64,
            ..Usage::default()
        },
        model: Model {
            name: oai.model,
            provider: ModelProvider::Custom(VLLM_KIND.to_string()),
            version: model_version,
            capabilities: Capabilities::default(),
        },
        status: PredictStatus::Success,
        metadata: finish,
    }
}

#[derive(Serialize)]
struct OaiCompletionRequest {
    model: String,
    prompt: String,
    max_tokens: u32,
    #[serde(skip_serializing_if = "Option::is_none")]
    temperature: Option<f32>,
    stream: bool,
}

#[derive(Deserialize)]
struct OaiCompletionResponse {
    model: String,
    choices: Vec<OaiChoice>,
    usage: OaiUsage,
}

#[derive(Deserialize)]
struct OaiChoice {
    text: String,
    finish_reason: Option<String>,
}

#[derive(Deserialize)]
struct OaiUsage {
    prompt_tokens: u32,
    completion_tokens: u32,
}

#[async_trait]
impl rskit_provider::Provider for VllmAdapter {
    fn name(&self) -> &'static str {
        VLLM_KIND
    }
}

#[async_trait]
impl rskit_provider::RequestResponse<PredictRequest, PredictResponse> for VllmAdapter {
    async fn execute(&self, input: PredictRequest) -> AppResult<PredictResponse> {
        self.predict(input).await.map_err(Into::into)
    }
}

#[async_trait]
impl Inference for VllmAdapter {
    async fn predict(&self, request: PredictRequest) -> Result<PredictResponse, InferenceError> {
        let body = vllm_completion_body(&self.config, &request);

        let req = Request::post("/v1/completions")
            .json_body(&body)
            .map_err(|err| InferenceError::Decode(format!("failed to build request: {err}")))?;

        let resp = self.client.send(req).await.map_err(InferenceError::from)?;
        if !resp.is_success() {
            let status = resp.status_u16();
            let body = resp.text_or_diagnostic();
            return Err(InferenceError::Server { status, body });
        }

        let text = resp
            .text()
            .map_err(|err| InferenceError::Decode(err.to_string()))?;
        let oai: OaiCompletionResponse =
            serde_json::from_str(&text).map_err(|err| InferenceError::Decode(err.to_string()))?;

        Ok(vllm_predict_response(oai, request.model_version))
    }

    fn descriptor(&self) -> InferenceDescriptor {
        InferenceDescriptor {
            name: VLLM_KIND.to_string(),
            description: "vLLM text generation via OAI-compatible /v1/completions".to_string(),
            serving_protocol: ServingProtocol::VllmRest,
            envelope: Envelope::default(),
        }
    }
}

#[async_trait]
impl StreamingInference for VllmAdapter {
    async fn predict_stream(
        &self,
        _request: PredictRequest,
    ) -> Result<Box<dyn Stream<Item = StreamEventRef> + Send + Unpin>, InferenceError> {
        Err(InferenceError::NotImplemented(
            "vLLM streaming is not implemented by this adapter yet",
        ))
    }
}

/// Explicitly register the vLLM adapter factory.
pub fn register(registry: &mut Registry, config: Config) -> Result<(), RegistryError> {
    let factory: Factory = Arc::new(move || Ok(Arc::new(VllmAdapter::new(config.clone())?)));
    registry.register(VLLM_KIND, factory)
}

#[async_trait]
impl Component for VllmAdapter {
    fn name(&self) -> &str {
        "rskit-inference.vllm"
    }

    async fn start(&self) -> AppResult<()> {
        Ok(())
    }

    async fn stop(&self) -> AppResult<()> {
        Ok(())
    }

    fn health(&self) -> Health {
        Health::healthy(self.name())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rskit_provider::RequestResponse as _;
    use std::collections::HashMap;

    #[test]
    fn vllm_descriptor() {
        let adapter = VllmAdapter::new(Config {
            base_url: "http://localhost:8000".into(),
            model: "llama3".into(),
            api_key: None,
            max_tokens: 256,
        })
        .unwrap();
        let desc = adapter.descriptor();
        assert_eq!(desc.name, VLLM_KIND);
        assert_eq!(desc.serving_protocol, ServingProtocol::VllmRest);
    }

    #[test]
    fn register_adds_vllm_kind() {
        let mut registry = Registry::new();
        register(
            &mut registry,
            Config {
                base_url: "http://localhost:8000".into(),
                model: "llama3".into(),
                api_key: None,
                max_tokens: 256,
            },
        )
        .expect("register vllm");
        assert!(registry.kinds().contains(&VLLM_KIND.to_string()));
    }

    #[test]
    fn config_defaults() {
        let config: Config =
            serde_json::from_str(r#"{"base_url":"http://localhost:8000"}"#).unwrap();
        assert_eq!(config.model, "default");
        assert_eq!(config.max_tokens, 256);
        assert!(config.api_key.is_none());
    }

    #[test]
    fn completion_body_uses_request_overrides() {
        let config = Config {
            base_url: "http://localhost:8000".into(),
            model: "configured".into(),
            api_key: Some(rskit_util::SecretString::new("secret")),
            max_tokens: 64,
        };
        let mut req = PredictRequest {
            model_name: "requested".to_owned(),
            inputs: HashMap::from([(
                "prompt".to_owned(),
                Value::Text {
                    text: "write".to_owned(),
                },
            )]),
            ..PredictRequest::default()
        };
        req.parameters
            .insert("max_tokens".to_owned(), serde_json::json!(9));
        req.parameters
            .insert("temperature".to_owned(), serde_json::json!(0.5));

        let body = serde_json::to_value(vllm_completion_body(&config, &req)).unwrap();

        assert_eq!(body["model"], "requested");
        assert_eq!(body["prompt"], "write");
        assert_eq!(body["max_tokens"], 9);
        assert_eq!(body["temperature"], 0.5);
        assert_eq!(body["stream"], false);
    }

    #[test]
    fn response_mapping_preserves_text_usage_and_finish_reason() {
        let response = vllm_predict_response(
            OaiCompletionResponse {
                model: "served".to_owned(),
                choices: vec![OaiChoice {
                    text: "answer".to_owned(),
                    finish_reason: Some("length".to_owned()),
                }],
                usage: OaiUsage {
                    prompt_tokens: 4,
                    completion_tokens: 5,
                },
            },
            Some("rev".to_owned()),
        );

        assert!(matches!(
            response.outputs.get("text"),
            Some(Value::Text { text }) if text == "answer"
        ));
        assert_eq!(
            response.metadata.get("finish_reason").map(String::as_str),
            Some("length")
        );
        assert_eq!(response.usage.output_tokens, 5);
        assert_eq!(response.model.version.as_deref(), Some("rev"));

        let empty = vllm_predict_response(
            OaiCompletionResponse {
                model: "served".to_owned(),
                choices: Vec::new(),
                usage: OaiUsage {
                    prompt_tokens: 0,
                    completion_tokens: 0,
                },
            },
            None,
        );
        assert!(matches!(
            empty.outputs.get("text"),
            Some(Value::Text { text }) if text.is_empty()
        ));
        assert!(empty.metadata.is_empty());
    }

    #[tokio::test]
    async fn provider_component_streaming_and_execute_fast_paths() {
        let adapter = VllmAdapter::new(Config {
            base_url: "http://127.0.0.1:1".into(),
            model: "test".into(),
            api_key: None,
            max_tokens: 64,
        })
        .unwrap();

        assert_eq!(rskit_provider::Provider::name(&adapter), VLLM_KIND);
        assert_eq!(Component::name(&adapter), "rskit-inference.vllm");
        adapter.start().await.unwrap();
        adapter.stop().await.unwrap();
        assert!(adapter.health().is_healthy());
        assert!(matches!(
            adapter.predict_stream(PredictRequest::default()).await,
            Err(InferenceError::NotImplemented(_))
        ));
        let err = adapter
            .execute(PredictRequest::default())
            .await
            .unwrap_err();
        assert!(matches!(
            err.code(),
            rskit_errors::ErrorCode::ExternalService | rskit_errors::ErrorCode::Internal
        ));
    }

    #[tokio::test]
    async fn predict_extracts_text_input() {
        let adapter = VllmAdapter::new(Config {
            base_url: "http://127.0.0.1:1".into(),
            model: "test".into(),
            api_key: None,
            max_tokens: 64,
        })
        .unwrap();

        let inputs = HashMap::from([("prompt".to_string(), Value::Text { text: "hi".into() })]);
        let req = PredictRequest {
            model_name: "test".into(),
            inputs,
            ..PredictRequest::default()
        };

        let err = adapter.predict(req).await.unwrap_err();
        assert!(
            matches!(
                err,
                InferenceError::Transport(_)
                    | InferenceError::Server { .. }
                    | InferenceError::Policy(_)
            ),
            "unexpected err: {err:?}"
        );
    }
}
