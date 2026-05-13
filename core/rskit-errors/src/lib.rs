//! Structured application error types with HTTP and gRPC status mapping.
//!
//! # Examples
//!
//! ```rust
//! use rskit_errors::{AppError, AppResult, ErrorCode};
//!
//! fn find_user(id: &str) -> AppResult<String> {
//!     Err(AppError::not_found("user", Some(id))
//!         .context("find_user"))
//! }
//!
//! let err = find_user("abc").unwrap_err();
//! assert_eq!(err.code, ErrorCode::NotFound);
//! assert!(err.message.contains("find_user"));
//! ```

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
pub use response::ProblemDetail;

/// Convenience alias used throughout rskit crates.
pub type AppResult<T> = Result<T, AppError>;
