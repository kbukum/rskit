//! Hugging Face Text Generation Inference (TGI) adapter via OAI-compatible endpoint.
//!
//! TGI exposes an OpenAI-compatible `/v1/chat/completions` endpoint from v2+.
//! Older deployments may only expose `/generate`; this adapter targets the
//! modern OAI-compatible path, which is the recommended production interface.
//!
//! Optional bearer credentials are configured through [`Config::api_key`] as a
//! redacting [`SecretString`](rskit_util::SecretString) and installed through `rskit-httpclient` auth
//! rather than raw headers.

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

pub(crate) use adapter::TgiAdapter;
pub(crate) use kind::TGI_KIND;
#[cfg(test)]
pub(crate) use protocol::{OaiChatChoice, OaiChatMessage, OaiUsage};
pub(crate) use protocol::{OaiChatResponse, tgi_chat_body, tgi_predict_response};
