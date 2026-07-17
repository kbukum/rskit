//! Convenience `Result` alias used throughout rskit crates.

use crate::AppError;

/// Convenience alias used throughout rskit crates.
pub type AppResult<T> = Result<T, AppError>;
