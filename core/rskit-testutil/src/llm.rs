//! Scriptable LLM provider fake for tests.
//!
//! [`FakeLlmProvider`] returns caller-scripted completion replies instead of calling a real LLM backend, so judge-metric and agent tests stay deterministic and offline. Enqueue a reply string to control the text a metric will parse (drive both well-formed and malformed/untrusted-output paths), an error to exercise provider-failure paths, or a hanging call to exercise timeout and cancellation.

use std::collections::VecDeque;
use std::time::Duration;

use async_trait::async_trait;
use parking_lot::Mutex;

use rskit_ai::{FinishReason, Usage, text_content};
use rskit_errors::{AppError, AppResult, ErrorCode};
use rskit_llm::types::AssistantMessage;
use rskit_llm::{CompletionRequest, CompletionResponse, Provider};

/// A scripted response for one [`FakeLlmProvider::complete`] call.
enum Script {
    /// Return this text as the assistant reply.
    Reply(String),
    /// Return this text as the assistant reply, reporting the given model as the one that generated it, so a caller's response-model handling can be exercised.
    ReplyAs(String, String),
    /// Return this text as the assistant reply, reporting the given finish reason, so a caller's completion-status validation (truncation, content filter, cancellation) can be exercised.
    ReplyWithFinish(FinishReason, String),
    /// Return this text as the assistant reply after the given delay, so tests can drive out-of-order completion under bounded concurrency.
    ReplyAfter(Duration, String),
    /// Return this error.
    Fail(AppError),
    /// Never resolve, so the caller's timeout/cancellation path is exercised.
    Hang,
}

/// A scriptable [`Provider`] fake that returns pre-configured completion replies.
///
/// Each enqueued script drives the next [`complete`](Provider::complete) call: [`will_reply`](Self::will_reply) yields the given text as the assistant message (use malformed, out-of-range, or injection-style text to drive a judge's untrusted-output handling), [`will_fail`](Self::will_fail) drives provider-failure paths, and [`will_hang`](Self::will_hang) never resolves so a caller's timeout and cancellation path can be exercised. The fake performs no network or model I/O and is fully deterministic.
pub struct FakeLlmProvider {
    scripts: Mutex<VecDeque<Script>>,
    calls: Mutex<usize>,
    last_request: Mutex<Option<CompletionRequest>>,
}

impl FakeLlmProvider {
    /// Creates a fake with no scripted responses.
    #[must_use]
    pub fn new() -> Self {
        Self {
            scripts: Mutex::new(VecDeque::new()),
            calls: Mutex::new(0),
            last_request: Mutex::new(None),
        }
    }

    /// Enqueues the assistant reply text returned by the next `complete` call.
    pub fn will_reply(&self, text: impl Into<String>) -> &Self {
        self.scripts.lock().push_back(Script::Reply(text.into()));
        self
    }

    /// Enqueues the assistant reply text returned by the next `complete` call, reporting `model` as the model that generated it rather than echoing the request, so a caller's response-model validation (for example, mixed-model rejection) can be driven.
    pub fn will_reply_as(&self, model: impl Into<String>, text: impl Into<String>) -> &Self {
        self.scripts
            .lock()
            .push_back(Script::ReplyAs(model.into(), text.into()));
        self
    }

    /// Enqueues the assistant reply text returned by the next `complete` call, reporting the given [`FinishReason`], so a caller's completion-status validation can be driven — for example a [`FinishReason::Length`] truncation or [`FinishReason::ContentFilter`] block whose body is still syntactically valid.
    pub fn will_reply_with_finish_reason(
        &self,
        reason: FinishReason,
        text: impl Into<String>,
    ) -> &Self {
        self.scripts
            .lock()
            .push_back(Script::ReplyWithFinish(reason, text.into()));
        self
    }

    /// Enqueues the assistant reply text returned by the next `complete` call after the given delay, so a test can force a later call to complete first.
    ///
    /// Uses [`tokio::time::sleep`], so tests started with `#[tokio::test(start_paused = true)]` auto-advance through the delay.
    pub fn will_reply_after(&self, delay: Duration, text: impl Into<String>) -> &Self {
        self.scripts
            .lock()
            .push_back(Script::ReplyAfter(delay, text.into()));
        self
    }

    /// Enqueues an error returned by the next `complete` call.
    pub fn will_fail(&self, err: AppError) -> &Self {
        self.scripts.lock().push_back(Script::Fail(err));
        self
    }

    /// Enqueues a `complete` call that never resolves, to exercise timeout paths.
    pub fn will_hang(&self) -> &Self {
        self.scripts.lock().push_back(Script::Hang);
        self
    }

    /// Returns how many `complete` calls have been recorded.
    #[must_use]
    pub fn call_count(&self) -> usize {
        *self.calls.lock()
    }

    /// Returns the most recent [`CompletionRequest`] passed to `complete`, so a test can
    /// assert how a caller shapes the request (model, `max_tokens`, temperature, messages).
    #[must_use]
    pub fn last_request(&self) -> Option<CompletionRequest> {
        self.last_request.lock().clone()
    }

    fn next_script(&self) -> Script {
        *self.calls.lock() += 1;
        self.scripts.lock().pop_front().unwrap_or_else(|| {
            Script::Fail(AppError::new(
                ErrorCode::Internal,
                "FakeLlmProvider: no scripted response enqueued",
            ))
        })
    }

    fn reply(request: CompletionRequest, text: String) -> CompletionResponse {
        Self::reply_as(request.model, text)
    }

    fn reply_as(model: String, text: String) -> CompletionResponse {
        Self::reply_finishing(model, text, FinishReason::Stop)
    }

    fn reply_finishing(
        model: String,
        text: String,
        stop_reason: FinishReason,
    ) -> CompletionResponse {
        CompletionResponse {
            message: AssistantMessage {
                content: text_content(text),
                tool_calls: Vec::new(),
                usage: None,
            },
            model,
            usage: Usage::default(),
            stop_reason: Some(stop_reason),
        }
    }
}

impl Default for FakeLlmProvider {
    fn default() -> Self {
        Self::new()
    }
}

impl rskit_provider::Provider for FakeLlmProvider {
    fn name(&self) -> &'static str {
        "fake_llm"
    }
}

#[async_trait]
impl rskit_provider::RequestResponse<CompletionRequest, CompletionResponse> for FakeLlmProvider {
    async fn execute(&self, input: CompletionRequest) -> AppResult<CompletionResponse> {
        self.complete(input).await
    }
}

#[async_trait]
impl Provider for FakeLlmProvider {
    async fn complete(&self, request: CompletionRequest) -> Result<CompletionResponse, AppError> {
        *self.last_request.lock() = Some(request.clone());
        match self.next_script() {
            Script::Reply(text) => Ok(Self::reply(request, text)),
            Script::ReplyAs(model, text) => Ok(Self::reply_as(model, text)),
            Script::ReplyWithFinish(reason, text) => {
                Ok(Self::reply_finishing(request.model, text, reason))
            }
            Script::ReplyAfter(delay, text) => {
                tokio::time::sleep(delay).await;
                Ok(Self::reply(request, text))
            }
            Script::Fail(err) => Err(err),
            Script::Hang => std::future::pending().await,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rskit_llm::types::user;

    fn request() -> CompletionRequest {
        CompletionRequest {
            model: "fake".into(),
            messages: vec![user("grade this")],
            max_tokens: None,
            temperature: None,
            stream: false,
            tools: None,
            tool_choice: None,
            ..Default::default()
        }
    }

    #[tokio::test]
    async fn returns_scripted_reply() {
        let provider = FakeLlmProvider::new();
        provider.will_reply("{\"score\":0.5}");
        let response = provider.complete(request()).await.expect("complete");
        assert_eq!(response.text(), "{\"score\":0.5}");
        assert_eq!(response.model, "fake");
        assert_eq!(provider.call_count(), 1);
    }

    #[tokio::test]
    async fn scripted_error_is_surfaced() {
        let provider = FakeLlmProvider::new();
        provider.will_fail(AppError::new(ErrorCode::ServiceUnavailable, "judge down"));
        let err = provider
            .complete(request())
            .await
            .expect_err("scripted error must surface");
        assert_eq!(err.code(), ErrorCode::ServiceUnavailable);
    }

    #[tokio::test]
    async fn missing_script_errors_instead_of_panicking() {
        let provider = FakeLlmProvider::new();
        let err = provider
            .complete(request())
            .await
            .expect_err("missing script must error");
        assert_eq!(err.code(), ErrorCode::Internal);
    }

    #[tokio::test]
    async fn hanging_call_never_resolves_within_timeout() {
        let provider = FakeLlmProvider::new();
        provider.will_hang();
        let elapsed =
            tokio::time::timeout(std::time::Duration::ZERO, provider.complete(request())).await;
        assert!(elapsed.is_err(), "hanging call must not resolve");
    }
}
