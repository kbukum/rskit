//! Media operation types for pipeline building.

mod compose;
/// Configuration for `ApplyFilter` operations.
pub mod filter_config;
/// Configuration for `Interpolate` operations.
pub mod interpolate;
/// Configuration for `AddOverlay` operations.
pub mod overlay_config;
/// Configuration for `DetectScenes` operations.
pub mod scene_detect;
mod spatial;
/// Configuration for `AddSubtitles` operations.
pub mod subtitle_config;
/// Configuration for `GenerateThumbnail` operations.
pub mod thumbnail;
/// Configuration for `Upscale` operations.
pub mod upscale;

pub use compose::*;
pub use filter_config::*;
pub use interpolate::*;
pub use overlay_config::*;
pub use scene_detect::*;
pub use spatial::*;
pub use subtitle_config::*;
pub use thumbnail::*;
pub use upscale::*;

use serde::{Deserialize, Serialize};

use crate::{
    filter::Filter,
    output::OutputConfig,
    subtitle::SubtitleTrack,
    time::{Segment, TimeRange},
    types::TrackKind,
};
use std::time::Duration;

/// A single media operation in a pipeline.
///
/// Each variant is a data-only description — no execution logic.
/// The pipeline records these and a backend executor compiles them.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[non_exhaustive]
pub enum MediaOp {
    // ── Temporal ─────────────────────────────────────────────────────
    /// Extract a time range from the source.
    Extract(TimeRange),
    /// Extract multiple segments and concatenate them.
    ExtractMany(Vec<Segment>),

    // ── Spatial (video/image) ────────────────────────────────────────
    /// Resize the video/image.
    Resize(ResizeOp),
    /// Crop the video/image.
    Crop(CropRegion),
    /// Rotate the video/image.
    Rotate(Rotation),
    /// Flip the video/image.
    Flip(FlipDirection),
    /// Pad the video/image to a target size.
    Pad(PadOp),

    // ── Speed / Time ─────────────────────────────────────────────────
    /// Change playback speed (e.g., 2.0 = double speed).
    Speed(f64),
    /// Reverse the media.
    Reverse,

    // ── Audio ────────────────────────────────────────────────────────
    /// Adjust volume (1.0 = unchanged, 0.5 = half, 2.0 = double).
    Volume(f64),
    /// Normalize audio loudness.
    NormalizeAudio,
    /// Fade in over the given duration.
    FadeIn(Duration),
    /// Fade out over the given duration.
    FadeOut(Duration),
    /// Remove the audio track.
    StripAudio,
    /// Remove the video track.
    StripVideo,

    // ── Filter (extensible) ──────────────────────────────────────────
    /// Apply a named filter.
    Filter(Filter),

    // ── Composition ──────────────────────────────────────────────────
    /// Overlay another source on top.
    Overlay(OverlayOp),
    /// Concatenate another source after this one.
    Concat(ConcatOp),
    /// Replace the audio track with another source.
    ReplaceAudio(ReplaceAudioOp),
    /// Mix another audio source on top.
    MixAudio(MixAudioOp),
    /// Burn subtitles into the video.
    BurnSubtitles(SubtitleTrack),

    // ── Track selection ──────────────────────────────────────────────
    /// Select specific tracks by index.
    SelectTracks(Vec<usize>),
    /// Select tracks by kind.
    SelectTracksByKind(Vec<TrackKind>),

    // ── Output ───────────────────────────────────────────────────────
    /// Transcode to a different format/codec.
    Transcode(OutputConfig),

    // ── Advanced filter / effects ────────────────────────────────────
    /// Apply a color grading or visual filter preset.
    ApplyFilter(FilterConfig),
    /// Add a text or image overlay on video.
    AddOverlay(OverlayConfig),
    /// Extract a single frame at a timestamp as an image.
    GenerateThumbnail(ThumbnailConfig),
    /// Detect scene boundaries and return timestamps.
    DetectScenes(SceneDetectConfig),
    /// Burn subtitles from an SRT/VTT/ASS source.
    AddSubtitles(SubtitleConfig),

    // ── AI-powered (external tools, not FFmpeg) ──────────────────────
    /// AI upscale using Real-ESRGAN.
    Upscale(UpscaleConfig),
    /// AI frame interpolation using RIFE.
    Interpolate(InterpolateConfig),
}
