//! Structured application error types with HTTP status mapping.
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
//! assert_eq!(err.code(), ErrorCode::NotFound);
//! assert!(err.message().contains("find_user"));
//! ```

#![warn(missing_docs)]

/// Machine-readable error classification codes.
pub mod code;
/// Conversions between [`AppError`] and HTTP status types.
pub mod convert;
/// Structured [`AppError`] type with rich context.
pub mod error;
/// RFC 9457 Problem Details response type.
pub mod response;
/// Convenience `Result` alias.
pub mod result;

pub use code::ErrorCode;
pub use error::AppError;
pub use response::{ProblemDetail, type_base_uri};
pub use result::AppResult;
