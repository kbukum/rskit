use crate::{AppError, ErrorCode};

// ── std::io::Error ──────────────────────────────────────────────────────────

impl From<std::io::Error> for AppError {
    fn from(e: std::io::Error) -> Self {
        AppError::new(ErrorCode::Internal, e.to_string())
    }
}

// ── serde_json::Error ───────────────────────────────────────────────────────

impl From<serde_json::Error> for AppError {
    fn from(e: serde_json::Error) -> Self {
        AppError::new(ErrorCode::InvalidFormat, e.to_string())
    }
}

// ── std::fmt::Error ─────────────────────────────────────────────────────────

impl From<std::fmt::Error> for AppError {
    fn from(e: std::fmt::Error) -> Self {
        AppError::new(ErrorCode::Internal, e.to_string())
    }
}

// ── std::str::Utf8Error ─────────────────────────────────────────────────────

impl From<std::str::Utf8Error> for AppError {
    fn from(e: std::str::Utf8Error) -> Self {
        AppError::new(ErrorCode::InvalidInput, e.to_string())
    }
}

// ── http::StatusCode ────────────────────────────────────────────────────────

impl From<&AppError> for http::StatusCode {
    fn from(e: &AppError) -> Self {
        e.http_status
    }
}
