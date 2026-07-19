//! Triton KServe v2 HTTP adapter for `rskit-inference`.
//!
//! This crate implements non-streaming [`rskit_inference::Inference`] against Triton's KServe v2 HTTP data plane.
//! KServe v2 HTTP has no native streaming protocol,
//! so this adapter intentionally does not implement [`rskit_inference::StreamingInference`].

#![warn(missing_docs)]

mod adapter;
mod config;
mod descriptor;
mod kind;
mod registration;
mod request;
mod response;
#[cfg(test)]
mod tests;

pub use config::Config;
pub use registration::register;

pub(crate) use adapter::TritonInference;
pub(crate) use descriptor::{authorize_prediction, descriptor_from_config};
pub(crate) use kind::TRITON_KIND;
pub(crate) use request::{encode_request, infer_path, operation_name};
#[cfg(test)]
pub(crate) use request::{encode_tensor, merged_wire_parameters};
#[cfg(test)]
pub(crate) use response::{
    TritonOutput, bytes_data, decode_output, decode_usage, numeric_f32, numeric_i64,
};
pub(crate) use response::{TritonResponse, decode_response};
