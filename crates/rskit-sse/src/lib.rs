//! Server-Sent Events bus with axum integration.
//!
//! [`SseBus`] is a typed, multi-subscriber broadcast bus that converts Tokio
//! broadcast channels into axum SSE response streams.

mod bus;

pub use bus::SseBus;
