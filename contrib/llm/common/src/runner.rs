//! Shared non-streaming chat adapter execution mechanics.

use std::future::Future;
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

use rskit_ai::semconv;
use rskit_errors::{AppError, AppResult, ErrorCode};
use rskit_httpclient::{HttpClient, Request};
use rskit_llm::types::{CompletionRequest, CompletionResponse};
use rskit_observability::{
    record_current_span_attribute, record_span_attribute, set_span_attribute,
};
use rskit_resilience::Policy;
use tracing::Instrument;

/// Shared runner for provider adapters that differ only by wire dialect.
#[derive(Clone)]
pub struct ChatRunner {
    system: &'static str,
    default_model: String,
    policy: Option<Policy>,
    last_call_at: Arc<AtomicU64>,
}

impl ChatRunner {
    /// Create a runner with the provider system name and default model.
    #[must_use]
    pub fn new(system: &'static str, default_model: impl Into<String>) -> Self {
        Self {
            system,
            default_model: default_model.into(),
            policy: None,
            last_call_at: Arc::new(AtomicU64::new(0)),
        }
    }

    /// Inject a resilience policy for outbound completion requests.
    #[must_use]
    pub fn with_policy(mut self, policy: Policy) -> Self {
        self.policy = Some(policy);
        self
    }

    /// Complete a request using provider-specific wire conversion.
    pub async fn complete<F, Fut>(
        &self,
        mut req: CompletionRequest,
        complete_once: F,
    ) -> AppResult<CompletionResponse>
    where
        F: Fn(CompletionRequest) -> Fut + Send + Sync,
        Fut: Future<Output = AppResult<CompletionResponse>> + Send,
    {
        if req.model.is_empty() {
            req.model.clone_from(&self.default_model);
        }

        let span = tracing::info_span!(
            "llm.complete",
            "gen_ai.system" = self.system,
            "gen_ai.operation.name" = semconv::Operation::Chat.as_str(),
            "gen_ai.request.model" = req.model.as_str(),
            "gen_ai.request.max_tokens" = tracing::field::Empty,
            "gen_ai.request.temperature" = tracing::field::Empty,
            "gen_ai.usage.input_tokens" = tracing::field::Empty,
            "gen_ai.usage.output_tokens" = tracing::field::Empty,
            "gen_ai.response.model" = tracing::field::Empty,
            "gen_ai.response.finish_reason" = tracing::field::Empty,
        );
        set_span_attribute(&span, semconv::SYSTEM, self.system);
        set_span_attribute(
            &span,
            semconv::OPERATION_NAME,
            semconv::Operation::Chat.as_str(),
        );
        set_span_attribute(&span, semconv::REQUEST_MODEL, req.model.clone());
        if let Some(max) = req.max_tokens {
            record_span_attribute(&span, semconv::REQUEST_MAX_TOKENS, i64::from(max));
        }
        if let Some(temp) = req.temperature {
            record_span_attribute(&span, semconv::REQUEST_TEMPERATURE, f64::from(temp));
        }

        let policy = self.policy.clone();
        async {
            let response = if let Some(policy) = policy {
                let req = req.clone();
                policy
                    .execute(|| {
                        let req = req.clone();
                        complete_once(req)
                    })
                    .await?
            } else {
                complete_once(req).await?
            };

            self.record_call();
            annotate_response(&response);
            Ok(response)
        }
        .instrument(span)
        .await
    }

    fn record_call(&self) {
        let now_ms = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_or(0, |duration| {
                u64::try_from(duration.as_millis()).unwrap_or(u64::MAX)
            });
        self.last_call_at.store(now_ms, Ordering::Relaxed);
    }
}

/// Send a provider request, map provider errors, and return the response body text.
pub async fn send_text(
    client: &HttpClient,
    request: Request,
    provider: &'static str,
    parse_error: impl FnOnce(u16, &str) -> AppError,
) -> AppResult<String> {
    let response = client.send(request).await?;

    if !response.is_success() {
        let status = response.status_u16();
        let text = response.text_or_diagnostic();
        return Err(parse_error(status, &text));
    }

    response.text().map_err(|error| {
        AppError::new(
            ErrorCode::ExternalService,
            format!("failed to read {provider} response: {error}"),
        )
    })
}

fn annotate_response(response: &CompletionResponse) {
    record_current_span_attribute(
        semconv::USAGE_INPUT_TOKENS,
        i64::try_from(response.usage.input_tokens).unwrap_or(i64::MAX),
    );
    record_current_span_attribute(
        semconv::USAGE_OUTPUT_TOKENS,
        i64::try_from(response.usage.output_tokens).unwrap_or(i64::MAX),
    );
    record_current_span_attribute(semconv::RESPONSE_MODEL, response.model.clone());
    if let Some(reason) = response.stop_reason.as_ref() {
        record_current_span_attribute(semconv::RESPONSE_FINISH_REASON, format!("{reason:?}"));
    }
}
