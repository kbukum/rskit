//! Structured application error types with HTTP and gRPC status mapping.

#![warn(missing_docs)]

/// Machine-readable error classification codes.
pub mod code;
/// Conversions between [`AppError`] and gRPC / HTTP status types.
pub mod convert;
/// Structured [`AppError`] type with rich context.
pub mod error;
/// RFC 9457 Problem Details response type.
pub mod response;

pub use code::ErrorCode;
pub use error::AppError;
pub use response::{ProblemDetail, set_type_base_uri, type_base_uri};

/// Convenience alias used throughout rskit crates.
pub type AppResult<T> = Result<T, AppError>;
