use std::sync::Arc;

use async_trait::async_trait;
use rskit_ai::semconv;
use rskit_authz::Decider;
use rskit_errors::AppResult;
use rskit_httpclient::{HttpClient, HttpClientConfig, Request};
use rskit_inference::{
    Inference, InferenceDescriptor, InferenceError, PredictRequest, PredictResponse,
};
use rskit_observability::{record_current_span_attribute, set_span_attribute};
use rskit_resilience::Policy;
use tracing::Instrument;

use crate::{
    Config, TRITON_KIND, TritonResponse, authorize_prediction, decode_response,
    descriptor_from_config, encode_request, infer_path, operation_name,
};

/// Triton KServe v2 HTTP inference adapter.
pub(crate) struct TritonInference {
    client: HttpClient,
    #[expect(dead_code, reason = "stored for future introspection; original design")]
    config: Config,
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
    pub(crate) fn new(config: Config) -> Result<Self, InferenceError> {
        let client = HttpClient::new(
            HttpClientConfig::new().with_base_url(config.base_url.trim_end_matches('/')),
        )?;
        Ok(Self::with_http_client(config, client))
    }

    /// Create a Triton adapter with an injected canonical HTTP client.
    #[must_use]
    pub(crate) fn with_http_client(config: Config, client: HttpClient) -> Self {
        let descriptor = descriptor_from_config(&config);
        let policy = config.policy.clone();
        let decider = config.decider.clone();
        Self {
            client,
            config,
            descriptor,
            policy,
            decider,
        }
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
            gen_ai.system = TRITON_KIND,
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
        set_span_attribute(&span, semconv::SYSTEM, TRITON_KIND);
        set_span_attribute(&span, semconv::OPERATION_NAME, operation.as_str());
        set_span_attribute(&span, semconv::REQUEST_MODEL, request.model_name.as_str());
        set_span_attribute(
            &span,
            semconv::REQUEST_MODEL_VERSION,
            model_version.as_str(),
        );
        set_span_attribute(&span, semconv::REQUEST_ID, request_id.as_str());

        async move {
            let response = self.predict_once(&request).await?;
            record_current_span_attribute(semconv::USAGE_INPUT_TOKENS, response.usage.input_tokens);
            record_current_span_attribute(
                semconv::USAGE_OUTPUT_TOKENS,
                response.usage.output_tokens,
            );
            record_current_span_attribute(
                semconv::USAGE_CACHED_TOKENS,
                response.usage.cached_tokens,
            );
            record_current_span_attribute(semconv::RESPONSE_MODEL, response.model.name.as_str());
            if let Some(reason) = response.metadata.get("finish_reason") {
                record_current_span_attribute(semconv::RESPONSE_FINISH_REASON, reason.as_str());
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
        let body = response.text().map_err(InferenceError::from)?;
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
