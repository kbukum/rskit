//! Composition operations for combining multiple media sources.

use rskit_file::FileSource;
use std::time::Duration;

use crate::time::{TimeRange, Timestamp};

/// Overlay another source on top of the current video.
#[derive(Debug, Clone)]
pub struct OverlayOp {
    /// The overlay source (image or video).
    pub source: FileSource,
    /// Where to place the overlay.
    pub position: OverlayPosition,
    /// Opacity (0.0 = transparent, 1.0 = opaque).
    pub opacity: f32,
    /// Time range during which the overlay is visible.
    pub time_range: Option<TimeRange>,
    /// Scale factor for the overlay.
    pub scale: Option<f64>,
}

/// Position for overlay placement.
#[derive(Debug, Clone)]
pub enum OverlayPosition {
    /// Offset from top-left corner.
    TopLeft(u32, u32),
    /// Offset from top-right corner.
    TopRight(u32, u32),
    /// Offset from bottom-left corner.
    BottomLeft(u32, u32),
    /// Offset from bottom-right corner.
    BottomRight(u32, u32),
    /// Centered on the video.
    Center,
    /// Custom position using expressions (e.g., FFmpeg expressions).
    Custom {
        /// X position expression.
        x: String,
        /// Y position expression.
        y: String,
    },
}

/// Concatenate another source after the current one.
#[derive(Debug, Clone)]
pub struct ConcatOp {
    /// Source to append.
    pub source: FileSource,
    /// Optional transition between segments.
    pub transition: Option<Transition>,
}

/// Transition between concatenated segments.
#[derive(Debug, Clone)]
pub enum Transition {
    /// Cross-fade between segments.
    CrossFade(Duration),
    /// Fade to black between segments.
    FadeToBlack(Duration),
    /// Hard cut (no transition).
    Cut,
}

/// Replace the audio track entirely.
#[derive(Debug, Clone)]
pub struct ReplaceAudioOp {
    /// New audio source.
    pub audio_source: FileSource,
    /// Offset into the video to start the new audio.
    pub offset: Option<Timestamp>,
}

/// Mix additional audio on top of existing audio.
#[derive(Debug, Clone)]
pub struct MixAudioOp {
    /// Audio source to mix in.
    pub audio_source: FileSource,
    /// Volume of the mixed audio (relative to original).
    pub volume: f64,
    /// Offset into the video to start mixing.
    pub offset: Option<Timestamp>,
}
