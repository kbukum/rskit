use rskit_errors::AppError;

/// The error returned when a non-interactive prompt has no usable default.
pub(crate) fn non_interactive_error(prompt: &str) -> AppError {
    AppError::invalid_input(
        "prompt",
        format!("non-interactive mode requires a default for: {prompt}"),
    )
}

/// The error returned when input closes before the prompt is answered.
pub(crate) fn closed_input(prompt: &str) -> AppError {
    AppError::invalid_input("prompt", format!("input closed before answering: {prompt}"))
}

/// The error returned when the user cancels an interactive prompt (Esc/Ctrl+C).
pub(crate) fn cancelled(prompt: &str) -> AppError {
    AppError::cancelled(format!("prompt cancelled: {prompt}"))
}
