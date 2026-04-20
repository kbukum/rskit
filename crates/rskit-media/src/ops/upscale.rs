//! Configuration types for the `Upscale` operation.

use serde::{Deserialize, Serialize};

/// Configuration for AI-based image/video upscaling.
///
/// This operation shells out to `realesrgan-ncnn-vulkan`, not FFmpeg.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UpscaleConfig {
    /// Which upscale model to use.
    pub model: UpscaleModel,
    /// Scaling factor (2 or 4).
    pub scale: u8,
    /// Denoise strength (0.0 = off, 1.0 = maximum). Only supported by some models.
    pub denoise_strength: Option<f32>,
}

/// Available upscale models for Real-ESRGAN.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[non_exhaustive]
pub enum UpscaleModel {
    /// General-purpose x4 upscaler.
    RealEsrganX4Plus,
    /// Anime-optimized upscaler.
    RealEsrganAnime,
    /// Video-optimized x4 model.
    RealEsrganX4Video,
}
