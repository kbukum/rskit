//! Error helpers for logging setup.

/// Result type returned by fallible logging setup APIs.
pub type LoggingResult<T> = rskit_errors::AppResult<T>;

#[cfg(feature = "setup")]
pub(crate) fn invalid_regex(
    pattern: impl Into<String>,
    cause: regex::Error,
) -> rskit_errors::AppError {
    use rskit_errors::{AppError, ErrorCode};
    let pattern = pattern.into();
    AppError::new(ErrorCode::InvalidFormat, "invalid masking regex pattern")
        .with_detail("pattern", pattern)
        .with_cause(cause)
}

#[cfg(feature = "setup")]
pub(crate) fn log_file_open(
    path: impl Into<String>,
    cause: std::io::Error,
) -> rskit_errors::AppError {
    let path = path.into();
    rskit_errors::AppError::from(cause)
        .context("open log output file")
        .with_detail("path", path)
}

#[cfg(feature = "otlp")]
pub(crate) fn invalid_protocol(protocol: impl Into<String>) -> rskit_errors::AppError {
    let protocol = protocol.into();
    rskit_errors::AppError::invalid_input("otlp.protocol", "expected `grpc` or `http`")
        .with_detail("protocol", protocol)
}

#[cfg(feature = "otlp")]
pub(crate) fn grpc_headers_not_supported() -> rskit_errors::AppError {
    rskit_errors::AppError::invalid_input(
        "otlp.headers",
        "custom headers are supported only with the `http` OTLP protocol",
    )
}

#[cfg(feature = "otlp")]
pub(crate) fn insecure_conflicts_with_endpoint(
    endpoint: impl Into<String>,
) -> rskit_errors::AppError {
    rskit_errors::AppError::invalid_input(
        "otlp.insecure",
        "`insecure = true` conflicts with an `https://` endpoint",
    )
    .with_detail("endpoint", endpoint.into())
}

#[cfg(feature = "otlp")]
pub(crate) fn secure_requires_https_endpoint(
    endpoint: impl Into<String>,
) -> rskit_errors::AppError {
    rskit_errors::AppError::invalid_input(
        "otlp.insecure",
        "an `http://` endpoint requires `insecure = true`; use `https://` for secure transport",
    )
    .with_detail("endpoint", endpoint.into())
}

#[cfg(feature = "otlp")]
pub(crate) fn otlp_exporter(
    cause: impl std::error::Error + Send + Sync + 'static,
) -> rskit_errors::AppError {
    rskit_errors::AppError::external_service("otlp", cause)
}

#[cfg(feature = "otlp")]
pub(crate) fn otlp_shutdown(
    cause: impl std::error::Error + Send + Sync + 'static,
) -> rskit_errors::AppError {
    rskit_errors::AppError::external_service("otlp", cause).context("shutdown logging exporter")
}
