use async_trait::async_trait;
use rskit_ai::StreamEventRef;
use rskit_component::{Component, Health};
use rskit_errors::AppResult;
use rskit_httpclient::{Auth, HttpClient, HttpClientConfig, Request};
use rskit_inference::{
    CapabilityHints, Inference, InferenceDescriptor, InferenceError, PredictRequest,
    PredictResponse, ServingProtocol, StreamingInference,
};
use rskit_tool::Envelope;
use tokio_stream::Stream;

use crate::{Config, OaiChatResponse, TGI_KIND, tgi_chat_body, tgi_predict_response};

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
            capabilities: CapabilityHints {
                supports_streaming: true,
                ..CapabilityHints::default()
            },
            available: true,
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
