//! Media types, codec/format registry, pipeline builder, and processing traits.
//!
//! `rskit-media` defines the vocabulary for media processing without any
//! processing logic. Backends (FFmpeg, image crate) implement the traits
//! defined here.

#![warn(missing_docs)]

/// Audio sample rate and channel layout types.
pub mod audio;
/// Codec identifiers, profiles, levels, and well-known constants.
pub mod codec;
/// Color space, color range, and pixel format types.
pub mod color;
/// Executor trait for media processing backends.
pub mod executor;
/// Filter types and convenience constructors.
pub mod filter;
/// Container/file format identifiers and well-known constants.
pub mod format;
/// Media operation types for pipeline building.
pub mod ops;
/// Output configuration, quality, and encoding settings.
pub mod output;
/// Pipeline builder for chaining media operations.
pub mod pipeline;
/// Preset output configurations for common formats.
pub mod presets;
/// Media probing traits and metadata types.
pub mod probe;
/// Codec/format compatibility registry.
pub mod registry;
/// Resolution and frame rate types.
pub mod spatial;
/// Subtitle types and SRT/VTT parsing.
pub mod subtitle;
/// Timestamp, time range, and segment types.
pub mod time;
/// Track and track info types.
pub mod track;
/// Core media type enumerations.
pub mod types;

pub use audio::{ChannelLayout, SampleRate};
pub use codec::{Codec, CodecKind, CodecLevel, CodecProfile};
pub use color::{ColorRange, ColorSpace, PixelFormat};
pub use executor::MediaExecutor;
pub use filter::{Filter, FilterTarget, ParamValue, Params};
pub use format::Format;
pub use ops::MediaOp;
pub use output::{
    AudioSettings, Bitrate, DashConfig, EncodingSpeed, HlsConfig, HlsPlaylistType, OutputConfig,
    Quality, RtmpConfig, StreamingConfig, VideoSettings,
};
pub use pipeline::{MediaPipeline, Progress};
pub use probe::{MediaMetadata, MediaProbe};
pub use registry::{CodecInfo, FormatInfo, Registry};
pub use spatial::{FrameRate, Resolution};
pub use subtitle::{SubtitleEntry, SubtitlePosition, SubtitleStyle, SubtitleTrack};
pub use time::{Segment, TimeRange, Timestamp};
pub use track::{
    AudioTrackInfo, ContentLightLevel, HdrFormat, HdrMetadata, MasteringDisplay, SubtitleTrackInfo,
    Track, VideoTrackInfo,
};
pub use types::{MediaType, TrackKind};
