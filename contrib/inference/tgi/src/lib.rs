//! Hugging Face Text Generation Inference (TGI) adapter via OAI-compatible endpoint.
//!
//! TGI exposes an OpenAI-compatible `/v1/chat/completions` endpoint from v2+.
//! Older deployments may only expose `/generate`; this adapter targets the
//! modern OAI-compatible path, which is the recommended production interface.
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

const TGI_KIND: &str = "tgi";

/// Configuration for the TGI OAI-compatible adapter.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Config {
    /// Base URL of the TGI server, for example `http://localhost:8080`.
    pub base_url: String,
    /// Default model identifier.
    #[serde(default = "default_model")]
    pub model: String,
    /// Optional bearer token for authenticated TGI deployments.
    ///
    /// The value is redacted in debug output and serialization.
    #[serde(default)]
    pub api_key: Option<SecretString>,
    /// Max new tokens for generation.
    #[serde(default = "default_max_tokens")]
    pub max_tokens: u32,
}

fn default_model() -> String {
    TGI_KIND.into()
}

fn default_max_tokens() -> u32 {
    256
}

/// TGI adapter using the OAI-compatible chat-completions endpoint.
pub(crate) struct TgiAdapter {
    client: HttpClient,
    config: Config,
}

impl TgiAdapter {
    /// Create a new TGI adapter from config.
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

fn tgi_chat_body(adapter: &Config, request: &PredictRequest) -> OaiChatRequest {
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
        adapter.model.clone()
    } else {
        request.model_name.clone()
    };

    let max_tokens = request
        .parameters
        .get("max_tokens")
        .and_then(serde_json::Value::as_u64)
        .map(|value| value as u32)
        .unwrap_or(adapter.max_tokens);

    let temperature = request
        .parameters
        .get("temperature")
        .and_then(serde_json::Value::as_f64)
        .map(|value| value as f32);

    OaiChatRequest {
        model,
        messages: vec![OaiMessage {
            role: "user".to_string(),
            content: prompt,
        }],
        max_tokens,
        temperature,
        stream: false,
    }
}

fn tgi_predict_response(oai: OaiChatResponse, model_version: Option<String>) -> PredictResponse {
    let generated = oai
        .choices
        .first()
        .and_then(|choice| choice.message.content.clone())
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
            provider: ModelProvider::Custom(TGI_KIND.to_string()),
            version: model_version,
            capabilities: Capabilities::default(),
        },
        status: PredictStatus::Success,
        metadata: finish,
    }
}

#[derive(Serialize)]
struct OaiChatRequest {
    model: String,
    messages: Vec<OaiMessage>,
    max_tokens: u32,
    #[serde(skip_serializing_if = "Option::is_none")]
    temperature: Option<f32>,
    stream: bool,
}

#[derive(Serialize)]
struct OaiMessage {
    role: String,
    content: String,
}

#[derive(Deserialize)]
struct OaiChatResponse {
    model: String,
    choices: Vec<OaiChatChoice>,
    usage: OaiUsage,
}

#[derive(Deserialize)]
struct OaiChatChoice {
    message: OaiChatMessage,
    finish_reason: Option<String>,
}

#[derive(Deserialize)]
struct OaiChatMessage {
    content: Option<String>,
}

#[derive(Deserialize)]
struct OaiUsage {
    prompt_tokens: u32,
    completion_tokens: u32,
}

#[async_trait]
impl rskit_provider::Provider for TgiAdapter {
    fn name(&self) -> &'static str {
        TGI_KIND
    }
}

#[async_trait]
impl rskit_provider::RequestResponse<PredictRequest, PredictResponse> for TgiAdapter {
    async fn execute(&self, input: PredictRequest) -> AppResult<PredictResponse> {
        self.predict(input).await.map_err(Into::into)
    }
}

#[async_trait]
impl Inference for TgiAdapter {
    async fn predict(&self, request: PredictRequest) -> Result<PredictResponse, InferenceError> {
        let body = tgi_chat_body(&self.config, &request);

        let req = Request::post("/v1/chat/completions")
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
        let oai: OaiChatResponse =
            serde_json::from_str(&text).map_err(|err| InferenceError::Decode(err.to_string()))?;

        Ok(tgi_predict_response(oai, request.model_version))
    }

    fn descriptor(&self) -> InferenceDescriptor {
        InferenceDescriptor {
            name: TGI_KIND.to_string(),
            description: "Hugging Face TGI text generation via OAI-compatible /v1/chat/completions"
                .to_string(),
            serving_protocol: ServingProtocol::TgiRest,
            envelope: Envelope::default(),
        }
    }
}

#[async_trait]
impl StreamingInference for TgiAdapter {
    async fn predict_stream(
        &self,
        _request: PredictRequest,
    ) -> Result<Box<dyn Stream<Item = StreamEventRef> + Send + Unpin>, InferenceError> {
        Err(InferenceError::NotImplemented(
            "TGI streaming is not implemented by this adapter yet",
        ))
    }
}

#[async_trait]
impl Component for TgiAdapter {
    fn name(&self) -> &str {
        "rskit-inference.tgi"
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

/// Explicitly register the TGI adapter factory.
pub fn register(registry: &mut Registry, config: Config) -> Result<(), RegistryError> {
    let factory: Factory = Arc::new(move || Ok(Arc::new(TgiAdapter::new(config.clone())?)));
    registry.register(TGI_KIND, factory)
}

#[cfg(test)]
mod tests {
    use super::*;
    use rskit_provider::RequestResponse as _;

    #[test]
    fn tgi_descriptor() {
        let adapter = TgiAdapter::new(Config {
            base_url: "http://localhost:8080".into(),
            model: "tiiuae/falcon-7b".into(),
            api_key: None,
            max_tokens: 256,
        })
        .unwrap();
        let desc = adapter.descriptor();
        assert_eq!(desc.name, TGI_KIND);
        assert_eq!(desc.serving_protocol, ServingProtocol::TgiRest);
    }

    #[test]
    fn register_adds_tgi_kind() {
        let mut registry = Registry::new();
        register(
            &mut registry,
            Config {
                base_url: "http://localhost:8080".into(),
                model: "tiiuae/falcon-7b".into(),
                api_key: None,
                max_tokens: 256,
            },
        )
        .expect("register tgi");
        assert!(registry.kinds().contains(&TGI_KIND.to_string()));
    }

    #[test]
    fn config_defaults() {
        let config: Config =
            serde_json::from_str(r#"{"base_url":"http://localhost:8080"}"#).unwrap();
        assert_eq!(config.model, "tgi");
        assert_eq!(config.max_tokens, 256);
        assert!(config.api_key.is_none());
    }

    #[test]
    fn request_body_uses_text_alias_defaults_and_parameters() {
        let config = Config {
            base_url: "http://localhost:8080".into(),
            model: "default-model".into(),
            api_key: Some(rskit_util::SecretString::new("secret")),
            max_tokens: 64,
        };
        let mut req = PredictRequest {
            model_name: String::new(),
            inputs: std::collections::HashMap::from([(
                "text".to_owned(),
                Value::Text {
                    text: "hello".to_owned(),
                },
            )]),
            ..PredictRequest::default()
        };
        req.parameters
            .insert("max_tokens".to_owned(), serde_json::json!(7));
        req.parameters
            .insert("temperature".to_owned(), serde_json::json!(0.25));

        let body = serde_json::to_value(tgi_chat_body(&config, &req)).unwrap();

        assert_eq!(body["model"], "default-model");
        assert_eq!(body["messages"][0]["role"], "user");
        assert_eq!(body["messages"][0]["content"], "hello");
        assert_eq!(body["max_tokens"], 7);
        assert_eq!(body["temperature"], 0.25);
        assert_eq!(body["stream"], false);
    }

    #[test]
    fn response_mapping_handles_finish_reason_and_empty_choices() {
        let response = tgi_predict_response(
            OaiChatResponse {
                model: "served".to_owned(),
                choices: vec![OaiChatChoice {
                    message: OaiChatMessage {
                        content: Some("done".to_owned()),
                    },
                    finish_reason: Some("stop".to_owned()),
                }],
                usage: OaiUsage {
                    prompt_tokens: 2,
                    completion_tokens: 3,
                },
            },
            Some("v1".to_owned()),
        );

        assert!(matches!(
            response.outputs.get("text"),
            Some(Value::Text { text }) if text == "done"
        ));
        assert_eq!(
            response.metadata.get("finish_reason").map(String::as_str),
            Some("stop")
        );
        assert_eq!(response.usage.input_tokens, 2);
        assert_eq!(response.model.version.as_deref(), Some("v1"));

        let empty = tgi_predict_response(
            OaiChatResponse {
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
        let adapter = TgiAdapter::new(Config {
            base_url: "http://127.0.0.1:1".into(),
            model: "test".into(),
            api_key: None,
            max_tokens: 64,
        })
        .unwrap();

        assert_eq!(rskit_provider::Provider::name(&adapter), TGI_KIND);
        assert_eq!(Component::name(&adapter), "rskit-inference.tgi");
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
    async fn predict_transport_error_on_bad_url() {
        let adapter = TgiAdapter::new(Config {
            base_url: "http://127.0.0.1:1".into(),
            model: "test".into(),
            api_key: None,
            max_tokens: 64,
        })
        .unwrap();

        let req = PredictRequest {
            model_name: "test".into(),
            inputs: std::collections::HashMap::from([(
                "prompt".to_string(),
                Value::Text {
                    text: "hello".into(),
                },
            )]),
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
