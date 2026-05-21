//! Model-serving runtime adapter layer for rskit.
//!
//! `rskit-inference` owns runtime-neutral prediction contracts for Triton,
//! vLLM raw, TGI, KServe v2, BentoML, ONNX Runtime Server, TFServing, and
//! custom REST/gRPC serving runtimes. It intentionally does not model chat
//! completions; chat belongs to `rskit-llm` and provider implementations under
//! `rskit-llm-providers`.
//!
//! Adapters declare their executable authority through `rskit_tool::Envelope` on
//! [`InferenceDescriptor`]. Consumers and orchestrators enforce envelope
//! intersection with the five-stage permission model (declaration, registry
//! verification, activation, per-invocation enforcement, HITL elicitation).

#![warn(missing_docs)]

/// Lean default echo adapter.
pub mod echo;
/// Model-serving inference traits.
pub mod inference;
/// Explicit adapter registry.
pub mod registry;
/// Runtime-neutral request, response, value, descriptor, and error types.
pub mod types;

pub use echo::{Echo, register as register_echo};
pub use inference::{Inference, StreamingInference};
pub use registry::{Factory, Registry, RegistryError, default_registry};
pub use rskit_ai::{Model, StreamEvent, StreamEventRef, Usage};
pub use types::{
    InferenceDescriptor, InferenceError, PredictRequest, PredictResponse, PredictStatus,
    ServingProtocol, Tensor, TensorData, Value,
};
