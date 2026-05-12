//! Configuration types for the `AddOverlay` operation.

use std::path::PathBuf;

use serde::{Deserialize, Serialize};

use crate::time::TimeRange;

/// Overlay configuration for adding text or image overlays to video.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OverlayConfig {
    /// Whether the overlay is text or an image.
    pub overlay_type: OverlayType,
    /// Normalized position on the video frame (0.0–1.0).
    pub position: Position,
    /// Normalized size (0.0–1.0 relative to video dimensions).
    pub size: Option<Size>,
    /// Opacity (0.0 = transparent, 1.0 = opaque).
    pub opacity: f32,
    /// Time range during which the overlay is visible.
    pub time_range: Option<TimeRange>,
}

/// The content of an overlay.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[non_exhaustive]
pub enum OverlayType {
    /// Text overlay with styling.
    Text(TextOverlay),
    /// Image overlay loaded from a file.
    Image(PathBuf),
}

/// Text overlay parameters.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TextOverlay {
    /// The text string to render.
    pub text: String,
    /// Font family name.
    pub font_family: Option<String>,
    /// Font size in points.
    pub font_size: Option<u16>,
    /// Text color (CSS format, e.g., "#FFFFFF").
    pub color: Option<String>,
}

/// Normalized (0.0–1.0) position on the video frame.
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub struct Position {
    /// Horizontal position (0.0 = left, 1.0 = right).
    pub x: f32,
    /// Vertical position (0.0 = top, 1.0 = bottom).
    pub y: f32,
}

/// Normalized (0.0–1.0) dimensions relative to video frame.
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub struct Size {
    /// Width (0.0–1.0).
    pub width: f32,
    /// Height (0.0–1.0).
    pub height: f32,
}
