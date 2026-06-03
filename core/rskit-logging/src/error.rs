//! Error helpers for logging setup.

use rskit_errors::{AppError, ErrorCode};

/// Result type returned by fallible logging setup APIs.
pub type LoggingResult<T> = rskit_errors::AppResult<T>;

pub(crate) fn invalid_regex(pattern: impl Into<String>, cause: regex::Error) -> AppError {
    let pattern = pattern.into();
    AppError::new(ErrorCode::InvalidFormat, "invalid masking regex pattern")
        .with_detail("pattern", pattern)
        .with_cause(cause)
}

pub(crate) fn log_file_open(path: impl Into<String>, cause: std::io::Error) -> AppError {
    let path = path.into();
    AppError::from(cause)
        .context("open log output file")
        .with_detail("path", path)
}

pub(crate) fn unsupported_output() -> AppError {
    AppError::invalid_input("logging.output", "unsupported log output type")
}

#[cfg(feature = "otlp")]
pub(crate) fn invalid_protocol(protocol: impl Into<String>) -> AppError {
    let protocol = protocol.into();
    AppError::invalid_input("otlp.protocol", "expected `grpc` or `http`")
        .with_detail("protocol", protocol)
}

#[cfg(feature = "otlp")]
pub(crate) fn grpc_headers_not_supported() -> AppError {
    AppError::invalid_input(
        "otlp.headers",
        "custom headers are supported only with the `http` OTLP protocol",
    )
}

#[cfg(feature = "otlp")]
pub(crate) fn otlp_exporter(cause: impl std::error::Error + Send + Sync + 'static) -> AppError {
    AppError::external_service("otlp", cause)
}

#[cfg(feature = "otlp")]
pub(crate) fn otlp_shutdown(cause: impl std::error::Error + Send + Sync + 'static) -> AppError {
    AppError::external_service("otlp", cause).context("shutdown logging exporter")
}
