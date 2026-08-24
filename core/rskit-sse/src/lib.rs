//! Server-Sent Events bus with axum integration.
//!
//! [`SseBus`] is a typed, bounded multi-subscriber bus with event IDs,
//! bounded replay for `Last-Event-ID`, and axum adapter streams.

#![warn(missing_docs)]

mod bus;

pub use bus::{SseBus, SseEvent};
