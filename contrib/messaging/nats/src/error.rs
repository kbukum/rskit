use rskit_errors::{AppError, ErrorCode};

pub(crate) fn nats_connect_error(error: impl std::fmt::Display) -> AppError {
    AppError::new(
        ErrorCode::ExternalService,
        format!("NATS connect failed: {error}"),
    )
}

pub(crate) fn nats_publish_error(error: impl std::fmt::Display) -> AppError {
    AppError::new(
        ErrorCode::ExternalService,
        format!("NATS publish failed: {error}"),
    )
}

pub(crate) fn nats_flush_error(error: impl std::fmt::Display) -> AppError {
    AppError::new(
        ErrorCode::ExternalService,
        format!("NATS flush failed: {error}"),
    )
}

pub(crate) fn nats_close_flush_error(error: impl std::fmt::Display) -> AppError {
    AppError::new(
        ErrorCode::ExternalService,
        format!("NATS flush before close failed: {error}"),
    )
}

pub(crate) fn nats_subscribe_error(error: impl std::fmt::Display) -> AppError {
    AppError::new(
        ErrorCode::ExternalService,
        format!("NATS subscribe failed: {error}"),
    )
}
