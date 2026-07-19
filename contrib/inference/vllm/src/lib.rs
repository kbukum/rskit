//! vLLM inference adapter using the OAI-compatible `/v1/completions` endpoint.
//!
//! Optional bearer credentials are configured through [`Config::api_key`] as a redacting [`SecretString`](rskit_util::SecretString)
//! and installed through `rskit-httpclient` auth rather than raw headers.

#![warn(missing_docs)]

mod adapter;
mod config;
mod kind;
mod protocol;
mod registration;
#[cfg(test)]
mod tests;

pub use config::Config;
pub use registration::register;

pub(crate) use adapter::VllmAdapter;
pub(crate) use kind::VLLM_KIND;
#[cfg(test)]
pub(crate) use protocol::{OaiChoice, OaiUsage};
pub(crate) use protocol::{OaiCompletionResponse, vllm_completion_body, vllm_predict_response};
