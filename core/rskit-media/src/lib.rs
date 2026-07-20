//! Media types, codec/format registry, pipeline builder, and processing traits.
//!
//! `rskit-media` defines the vocabulary for media processing without any processing logic.
//! Backends (FFmpeg, image crate) implement the traits defined here.

#![warn(missing_docs)]

// Concern groupings: the encoding and stream vocabularies live in their own
// folders. Their modules are re-exported at the crate root below so the public
// paths stay `rskit_media::codec`, `rskit_media::time`, and so on.
mod encoding;
mod stream;

/// Chunked media processing — split, process in parallel, reassemble.
pub mod chunking;
/// Executor trait for media processing backends.
pub mod executor;
/// Filter types and convenience constructors.
pub mod filter;
/// Media operation types for pipeline building.
pub mod ops;
/// Pipeline builder for chaining media operations.
pub mod pipeline;
/// Media probing traits and metadata types.
pub mod probe;
/// Duration-aware timeout calculation for media operations.
pub mod timeout;

/// Codec identifiers, profiles, levels, and well-known constants.
pub use encoding::codec;
/// Color space, color range, and pixel format types.
pub use encoding::color;
/// Container/file format identifiers and well-known constants.
pub use encoding::format;
/// Output configuration, quality, and encoding settings.
pub use encoding::output;
/// Preset output configurations for common formats.
pub use encoding::presets;
/// Codec/format compatibility registry.
pub use encoding::registry;
/// Audio sample rate and channel layout types.
pub use stream::audio;
/// Resolution and frame rate types.
pub use stream::spatial;
/// Subtitle types and SRT/VTT parsing.
pub use stream::subtitle;
/// Timestamp, time range, and segment types.
pub use stream::time;
/// Track and track info types.
pub use stream::track;
/// Core media type enumerations.
pub use stream::types;

pub use audio::{ChannelLayout, SampleRate};
pub use chunking::{
    ChunkBoundary, ChunkId, ChunkPlan, ChunkProgress, ChunkResult, ChunkStatus, ChunkStrategy,
    ChunkedOperation, FixedDurationStrategy, KeyframeStrategy, ReassemblyPlan, SilenceStrategy,
};
pub use codec::{Codec, CodecKind, CodecLevel, CodecProfile};
pub use color::{ColorRange, ColorSpace, PixelFormat};
pub use executor::MediaExecutor;
pub use filter::{Filter, FilterTarget, ParamValue, Params};
pub use format::Format;
pub use ops::{
    ColorAdjustments, FilterConfig, FilterPreset, ImageFormat, InterpolateConfig, InterpolateModel,
    MediaOp, OverlayConfig, OverlayType, Position, SceneDetectConfig, SceneDetectMethod, Size,
    SubtitleConfig, SubtitleFormat, SubtitleSource, TextOverlay, ThumbnailConfig, UpscaleConfig,
    UpscaleModel,
};
pub use output::{
    AudioSettings, Bitrate, DashConfig, EncodingSpeed, HlsConfig, HlsPlaylistType, OutputConfig,
    Quality, RtmpConfig, StreamingConfig, VideoSettings,
};
pub use pipeline::{MediaPipeline, Progress};
pub use probe::{Chapter, KeyframeInfo, MediaMetadata, MediaProbe, PictureType, SilenceInterval};
pub use registry::{CodecInfo, ExecutorFactory, FormatInfo, ProbeFactory, Registry};
pub use spatial::{FrameRate, Resolution};
pub use subtitle::{SubtitleEntry, SubtitlePosition, SubtitleStyle, SubtitleTrack};
pub use time::{Segment, TimeRange, Timestamp};
pub use timeout::{OperationKind, TimeoutCalculator};
pub use track::{
    AudioTrackInfo, ContentLightLevel, HdrFormat, HdrMetadata, MasteringDisplay, SubtitleTrackInfo,
    Track, VideoTrackInfo,
};
pub use types::{MediaType, TrackKind};
