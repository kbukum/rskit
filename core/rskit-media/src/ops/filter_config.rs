//! Configuration types for the `ApplyFilter` operation.

use serde::{Deserialize, Serialize};

/// Color grading / visual filter preset.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FilterConfig {
    /// Which preset to apply.
    pub preset: FilterPreset,
    /// Intensity of the filter effect (0.0 = off, 1.0 = full).
    pub intensity: f32,
    /// Optional custom color adjustments (used when `preset` is `Custom`).
    pub custom_params: Option<ColorAdjustments>,
}

/// Named color-grading presets.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[non_exhaustive]
pub enum FilterPreset {
    /// Warm cinematic look with crushed shadows.
    Cinematic,
    /// Warm tone shift.
    Warm,
    /// Cool / blue tone shift.
    Cool,
    /// Desaturated, warm tint reminiscent of old film stock.
    Vintage,
    /// High-contrast dramatic look.
    Dramatic,
    /// Black-and-white (grayscale).
    BW,
    /// User-defined adjustments via [`ColorAdjustments`].
    Custom,
}

/// Fine-grained color/brightness adjustments.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ColorAdjustments {
    /// Brightness offset (-1.0 to 1.0).
    pub brightness: Option<f32>,
    /// Contrast adjustment (-1.0 to 1.0).
    pub contrast: Option<f32>,
    /// Saturation adjustment (-1.0 to 1.0).
    pub saturation: Option<f32>,
    /// Temperature shift (-1.0 = cool, 1.0 = warm).
    pub temperature: Option<f32>,
    /// Gamma correction (0.1 to 10.0).
    pub gamma: Option<f32>,
}
