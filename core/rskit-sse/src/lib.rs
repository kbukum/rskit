//! Server-Sent Events bus with axum integration.
//!
//! [`SseBus`] is a typed, multi-subscriber broadcast bus that converts Tokio
//! broadcast channels into axum SSE response streams.
//! Delivery is forward-only: subscribers receive new events after subscription,
//! while replay, `Last-Event-ID`, heartbeat, and retry semantics remain owned by
//! the application handler composing the SSE response.

mod bus;

pub use bus::SseBus;
