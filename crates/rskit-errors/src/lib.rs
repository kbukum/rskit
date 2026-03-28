//! Structured application error types with HTTP and gRPC status mapping.

#![warn(missing_docs)]

/// Machine-readable error classification codes.
pub mod code;
/// Structured [`AppError`] type with rich context.
pub mod error;
/// Conversions between [`AppError`] and gRPC / HTTP status types.
pub mod convert;

pub use code::ErrorCode;
pub use error::AppError;

/// Convenience alias used throughout rskit crates.
pub type AppResult<T> = Result<T, AppError>;
