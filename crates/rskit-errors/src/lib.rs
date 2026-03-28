pub mod code;
pub mod error;
pub mod convert;

pub use code::ErrorCode;
pub use error::AppError;

/// Convenience alias used throughout rskit crates.
pub type AppResult<T> = Result<T, AppError>;
