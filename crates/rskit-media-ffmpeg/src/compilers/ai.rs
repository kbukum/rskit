//! Compilers for AI-powered operations (Upscale, Interpolate) — not FFmpeg-native.

use rskit_errors::AppResult;

pub(crate) fn compile_upscale() -> AppResult<()> {
    Err(rskit_errors::AppError::new(
        rskit_errors::ErrorCode::InvalidInput,
        "Upscale requires realesrgan-ncnn-vulkan, not FFmpeg",
    ))
}

pub(crate) fn compile_interpolate() -> AppResult<()> {
    Err(rskit_errors::AppError::new(
        rskit_errors::ErrorCode::InvalidInput,
        "Interpolate requires rife-ncnn-vulkan, not FFmpeg",
    ))
}
