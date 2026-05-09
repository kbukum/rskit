use async_trait::async_trait;
use tokio_stream::Stream;

use crate::types::{
    InferenceDescriptor, InferenceError, PredictRequest, PredictResponse, StreamEventRef,
};

/// Inference is the model-serving runtime trait. Adapters cover text
/// generation (vLLM, TGI), classification/regression (Triton KServe v2),
/// embeddings (BentoML, custom), image/audio inference, and arbitrary tensor
/// protocols. NOT chat completion — that lives in `rskit-llm`.
#[async_trait]
pub trait Inference: Send + Sync {
    /// Execute one prediction request against the serving runtime.
    async fn predict(&self, request: PredictRequest) -> Result<PredictResponse, InferenceError>;

    /// Return the adapter descriptor and executable permission envelope.
    fn descriptor(&self) -> InferenceDescriptor;
}

/// Implemented by serving runtimes that emit incremental output (vLLM, TGI).
#[async_trait]
pub trait StreamingInference: Inference {
    /// Execute a prediction request and stream incremental output events.
    async fn predict_stream(
        &self,
        request: PredictRequest,
    ) -> Result<Box<dyn Stream<Item = StreamEventRef> + Send + Unpin>, InferenceError>;
}
