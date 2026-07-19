//! Embedding provider trait definition.

use async_trait::async_trait;
use rskit_errors::AppResult;

use crate::{EmbedRequest, EmbedResponse};

/// Trait for canonical multimodal embedding providers.
///
/// Extends [`rskit_provider::RequestResponse<EmbedRequest, EmbedResponse>`]
/// so any embedding provider can be plugged directly into pipeline / dag / worker flows.
#[async_trait]
pub trait Provider: rskit_provider::RequestResponse<EmbedRequest, EmbedResponse> {
    /// Generate embeddings for one request.
    async fn embed(&self, req: EmbedRequest) -> AppResult<EmbedResponse>;

    /// Generate embeddings for a caller-controlled batch of requests.
    async fn embed_batch(&self, reqs: Vec<EmbedRequest>) -> AppResult<Vec<EmbedResponse>>;
}
