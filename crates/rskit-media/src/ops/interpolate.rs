//! Configuration types for the `Interpolate` operation.

use serde::{Deserialize, Serialize};

/// Configuration for AI-based frame interpolation.
///
/// This operation shells out to `rife-ncnn-vulkan`, not FFmpeg.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InterpolateConfig {
    /// Which interpolation model to use.
    pub model: InterpolateModel,
    /// Frame rate multiplier (2, 4, or 8).
    pub multiplier: u8,
}

/// Available frame interpolation models.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[non_exhaustive]
pub enum InterpolateModel {
    /// Standard RIFE model.
    Rife,
    /// High-definition RIFE model.
    RifeHD,
}
