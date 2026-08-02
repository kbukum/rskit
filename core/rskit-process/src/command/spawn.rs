//! Typed classification of subprocess spawn failures.

use std::io;

use crate::AppError;

/// Turn a subprocess spawn failure into a typed [`AppError`].
///
/// The OS detail is kept as the visible `{context}: {error}` message, while the
/// code is classified from the underlying [`io::ErrorKind`] (a missing program
/// becomes [`NotFound`](rskit_errors::ErrorCode::NotFound), a non-executable one
/// [`Forbidden`](rskit_errors::ErrorCode::Forbidden), and so on). Callers can
/// then tell "not installed" apart from other spawn failures instead of seeing
/// every failure collapsed into `Internal`. The classified error is preserved as
/// the cause so the original `io::Error` chain survives.
pub(crate) fn spawn_error(context: &str, error: io::Error) -> AppError {
    let message = format!("{context}: {error}");
    let classified = AppError::from(error);
    AppError::new(classified.code(), message).with_cause(classified)
}
