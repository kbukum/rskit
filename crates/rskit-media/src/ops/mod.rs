//! Media operation types for pipeline building.

mod compose;
mod spatial;

pub use compose::*;
pub use spatial::*;

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
}
