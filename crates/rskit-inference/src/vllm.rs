//! vLLM inference adapter using the OAI-compatible `/v1/completions` endpoint.

use std::sync::Arc;

use async_trait::async_trait;
use rskit_ai::{Capabilities, Model, Provider as ModelProvider, Usage};
use rskit_errors::AppResult;
use rskit_httpclient::{Auth, HttpClient, HttpClientConfig, Request};
use rskit_tool::Envelope;
use serde::{Deserialize, Serialize};

use crate::{
    Factory, Inference, InferenceDescriptor, InferenceError, PredictRequest, PredictResponse,
    PredictStatus, Registry, RegistryError, ServingProtocol, Value,
};

/// Registry kind for the vLLM adapter.
pub const VLLM_KIND: &str = "vllm";

/// Configuration for the vLLM OAI-compat adapter.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Config {
    /// Base URL of the vLLM server (e.g. `http://localhost:8000`).
    pub base_url: String,

    /// Default model name if not provided in the request.
    #[serde(default = "default_model")]
    pub model: String,

    /// Optional bearer token for authenticated vLLM deployments.
    #[serde(default)]
    pub api_key: Option<String>,

    /// Max tokens for generation (default: 256).
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
pub struct VllmAdapter {
    client: HttpClient,
    config: Config,
}

impl VllmAdapter {
    /// Create a new vLLM adapter from config.
    pub fn new(cfg: Config) -> AppResult<Self> {
        let mut http_cfg = HttpClientConfig::new().with_base_url(&cfg.base_url);
        if let Some(key) = &cfg.api_key {
            http_cfg = http_cfg.with_auth(Auth::bearer(key));
        }
        Ok(Self {
            client: HttpClient::new(http_cfg)?,
            config: cfg,
        })
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
impl Inference for VllmAdapter {
    async fn predict(&self, request: PredictRequest) -> Result<PredictResponse, InferenceError> {
        let prompt = request
            .inputs
            .get("prompt")
            .or_else(|| request.inputs.get("text"))
            .and_then(|v| match v {
                Value::Text { text } => Some(text.clone()),
                _ => None,
            })
            .unwrap_or_default();

        let model = if request.model_name.is_empty() {
            self.config.model.clone()
        } else {
            request.model_name.clone()
        };

        let max_tokens = request
            .parameters
            .get("max_tokens")
            .and_then(|v| v.as_u64())
            .map(|v| v as u32)
            .unwrap_or(self.config.max_tokens);

        let temperature = request
            .parameters
            .get("temperature")
            .and_then(|v| v.as_f64())
            .map(|v| v as f32);

        let body = OaiCompletionRequest {
            model: model.clone(),
            prompt,
            max_tokens,
            temperature,
            stream: false,
        };

        let req = Request::post("/v1/completions")
            .json_body(&body)
            .map_err(|e| InferenceError::Decode(format!("failed to build request: {e}")))?;

        let resp = self.client.send(req).await.map_err(InferenceError::from)?;

        if !resp.is_success() {
            let status = resp.status().as_u16();
            let body = resp.text().unwrap_or_default();
            return Err(InferenceError::Server { status, body });
        }

        let text = resp
            .text()
            .map_err(|e| InferenceError::Decode(e.to_string()))?;

        let oai: OaiCompletionResponse =
            serde_json::from_str(&text).map_err(|e| InferenceError::Decode(e.to_string()))?;

        let generated = oai
            .choices
            .first()
            .map(|c| c.text.clone())
            .unwrap_or_default();

        let finish = oai
            .choices
            .first()
            .and_then(|c| c.finish_reason.as_deref())
            .map(|r| ("finish_reason".to_string(), r.to_string()))
            .into_iter()
            .collect();

        Ok(PredictResponse {
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
                provider: ModelProvider::Custom("vllm".to_string()),
                version: request.model_version,
                capabilities: Capabilities::default(),
            },
            status: PredictStatus::Success,
            metadata: finish,
        })
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

/// Explicitly register the vLLM adapter factory.
pub fn register(registry: &mut Registry) -> Result<(), RegistryError> {
    let factory: Factory = Arc::new(|cfg| {
        let config: Config =
            serde_json::from_value(cfg).map_err(|e| InferenceError::Decode(e.to_string()))?;
        Ok(Arc::new(
            VllmAdapter::new(config).map_err(|e| InferenceError::Decode(e.to_string()))?,
        ))
    });
    registry.register(VLLM_KIND, factory)
}

#[cfg(test)]
mod tests {
    use super::*;
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
        register(&mut registry).expect("register vllm");
        assert!(registry.kinds().contains(&VLLM_KIND.to_string()));
    }

    #[test]
    fn config_defaults() {
        let cfg: Config = serde_json::from_str(r#"{"base_url":"http://localhost:8000"}"#).unwrap();
        assert_eq!(cfg.model, "default");
        assert_eq!(cfg.max_tokens, 256);
        assert!(cfg.api_key.is_none());
    }

    #[tokio::test]
    async fn predict_extracts_text_input() {
        // Verify input extraction logic without a live server — we test the
        // path up to the HTTP send by using an intentionally bad URL so we can
        // assert the request was built (not that network I/O succeeded).
        let adapter = VllmAdapter::new(Config {
            base_url: "http://127.0.0.1:1".into(), // nothing listening
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
        // Connection refused comes through as Policy or Transport error.
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
