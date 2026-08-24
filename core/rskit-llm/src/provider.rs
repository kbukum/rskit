//! Provider trait — the canonical abstraction over LLM backends.
//!
//! This is the single full LLM provider trait. Implementors supply [`Provider::complete`],
//! [`rskit_provider::Provider::name`],
//! and [`rskit_provider::RequestResponse::execute`] (which typically delegates to `complete`).
//!
//! The trait extends `rskit_provider::RequestResponse<CompletionRequest, CompletionResponse>`
//! so any LLM provider is natively usable in `dag`, `pipeline`, `chain`, `worker`,
//! and `process` consumers without adapter shims.

use std::pin::Pin;
use std::sync::Arc;

use async_trait::async_trait;
use futures::Stream as FutStream;
use rskit_ai::chat::{Message, count_tokens_approx};
use rskit_ai::{
    Capabilities, FinishReason, MessageStart, MessageStop, Role, StreamEventRef, TextDelta,
    UsageDelta, text_of,
};
use rskit_errors::{AppError, AppResult};

use crate::types::{CompletionRequest, CompletionResponse};

/// A fully-featured LLM provider with streaming and capability introspection.
///
/// An adapter MUST implement [`Provider::complete`],
/// [`rskit_provider::Provider::name`] (`&'static str`),
/// and [`rskit_provider::RequestResponse::execute`] (typically delegates to `complete`).
/// The default [`Provider::stream`] synthesizes a four-event sequence (`message.start` → `text.delta` → `usage.delta` → `message.stop`) by awaiting `complete`.
/// Adapters whose backend supports native streaming SHOULD override `stream` to emit incremental events.
///
/// # Native provider shape
///
/// This trait requires `rskit_provider::RequestResponse<CompletionRequest, CompletionResponse>` as supertrait,
/// so every `llm::Provider` carries the canonical identity/availability + request/response contract natively.
/// The optional [`LlmStream`] wrapper remains available for consumers that specifically need the provider `Stream` shape.
#[async_trait]
pub trait Provider: rskit_provider::RequestResponse<CompletionRequest, CompletionResponse> {
    /// Send a chat completion request and return the full response.
    async fn complete(&self, request: CompletionRequest) -> Result<CompletionResponse, AppError>;

    /// Stream a chat completion as a series of stream event objects.
    ///
    /// Default impl synthesizes events from [`Provider::complete`] for adapters whose backend has no native streaming endpoint.
    async fn stream(
        &self,
        request: CompletionRequest,
    ) -> Result<Pin<Box<dyn FutStream<Item = StreamEventRef> + Send>>, AppError> {
        let resp = self.complete(request).await?;
        let text = text_of(&resp.message.content);
        let model = resp.model.clone();
        let usage = resp.usage;
        let finish_reason = resp.stop_reason.unwrap_or(FinishReason::Stop);
        let mut events: Vec<StreamEventRef> = Vec::with_capacity(4);
        events.push(Arc::new(MessageStart {
            role: Role::Assistant,
            model,
            request_id: None,
        }));
        if !text.is_empty() {
            events.push(Arc::new(TextDelta { text }));
        }
        events.push(Arc::new(UsageDelta { usage }));
        events.push(Arc::new(MessageStop { finish_reason }));
        Ok(Box::pin(futures::stream::iter(events)))
    }

    /// Describe what this provider / model supports. Default returns an empty [`Capabilities`];
    /// adapters SHOULD override to advertise tool use, streaming, vision, etc.
    fn capabilities(&self) -> Capabilities {
        Capabilities::default()
    }

    /// Estimate the number of tokens consumed by the given messages.
    /// Default uses the shared whitespace-based approximation from `rskit_ai::chat`.
    fn count_tokens(&self, messages: &[Message]) -> usize {
        count_tokens_approx(messages)
    }
}

/// Adapter wrapping an `llm::Provider` as `provider::RequestResponse<CompletionRequest, CompletionResponse>`.
///
/// Use this to plug an LLM provider directly into pipeline/dag/chain consumers.
pub struct LlmRequestResponse<P: Provider>(pub Arc<P>);

#[async_trait]
impl<P: Provider + 'static> rskit_provider::Provider for LlmRequestResponse<P> {
    fn name(&self) -> &'static str {
        self.0.name()
    }
}

#[async_trait]
impl<P: Provider + 'static> rskit_provider::RequestResponse<CompletionRequest, CompletionResponse>
    for LlmRequestResponse<P>
{
    async fn execute(&self, input: CompletionRequest) -> AppResult<CompletionResponse> {
        self.0.complete(input).await
    }
}

/// Type alias for the provider-shaped boxed stream (mirrors `rskit_provider::traits::BoxStream`).
type ProviderBoxStream<O> = Pin<Box<dyn FutStream<Item = AppResult<O>> + Send + 'static>>;

/// Adapter wrapping an `llm::Provider` as `provider::Stream<CompletionRequest, StreamEventRef>`.
///
/// Use this to plug an LLM provider's streaming into pipeline/dag consumers.
pub struct LlmStream<P: Provider>(pub Arc<P>);

#[async_trait]
impl<P: Provider + 'static> rskit_provider::Provider for LlmStream<P> {
    fn name(&self) -> &'static str {
        self.0.name()
    }
}

impl<P: Provider + 'static> rskit_provider::Stream<CompletionRequest, StreamEventRef>
    for LlmStream<P>
{
    async fn execute(
        &self,
        input: CompletionRequest,
    ) -> AppResult<ProviderBoxStream<StreamEventRef>> {
        use futures::StreamExt;
        let raw = Provider::stream(&*self.0, input).await?;
        Ok(Box::pin(raw.map(Ok)) as ProviderBoxStream<StreamEventRef>)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types;

    #[test]
    fn test_capabilities_default() {
        let cap = Capabilities::default();
        assert!(!cap.tool_use);
        assert!(!cap.vision);
        assert!(!cap.reasoning_tokens);
        assert!(!cap.streaming);
        assert_eq!(cap.max_input_tokens.unwrap_or_default(), 0);
        assert!(cap.max_output_tokens.is_none());
    }

    #[test]
    fn test_count_tokens_approx_user() {
        let msgs = vec![types::user("hello world")];
        assert!(count_tokens_approx(&msgs) > 0);
    }

    #[test]
    fn test_count_tokens_approx_empty() {
        let msgs: Vec<Message> = vec![];
        assert_eq!(count_tokens_approx(&msgs), 0);
    }

    // Provider-driven behaviour (complete, default stream synthesis, count_tokens,
    // and the `LlmRequestResponse` adapter) is proven against the shipped `Echo`
    // provider in `echo.rs`, so no bespoke provider double is duplicated here.
}
