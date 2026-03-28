//! Media types, codec/format registry, pipeline builder, and processing traits.
//!
//! `rskit-media` defines the vocabulary for media processing without any
//! processing logic. Backends (FFmpeg, image crate) implement the traits
//! defined here.

#![warn(missing_docs)]

/// Core media type enumerations.
pub mod types;
/// Timestamp, time range, and segment types.
pub mod time;
/// Resolution and frame rate types.
pub mod spatial;
/// Audio sample rate and channel layout types.
pub mod audio;
/// Track and track info types.
pub mod track;
/// Codec identifiers and well-known constants.
pub mod codec;
/// Container/file format identifiers and well-known constants.
pub mod format;
/// Codec/format compatibility registry.
pub mod registry;
/// Filter types and convenience constructors.
pub mod filter;
/// Output configuration, quality, and encoding settings.
pub mod output;
/// Preset output configurations for common formats.
pub mod presets;
/// Media probing traits and metadata types.
pub mod probe;
/// Media operation types for pipeline building.
pub mod ops;
/// Pipeline builder for chaining media operations.
pub mod pipeline;
/// Executor trait for media processing backends.
pub mod executor;
/// Subtitle types and SRT/VTT parsing.
pub mod subtitle;

pub use types::{MediaType, TrackKind};
pub use time::{Timestamp, TimeRange, Segment};
pub use spatial::{Resolution, FrameRate};
pub use audio::{SampleRate, ChannelLayout};
pub use track::{Track, VideoTrackInfo, AudioTrackInfo, SubtitleTrackInfo};
pub use codec::{Codec, CodecKind};
pub use format::Format;
pub use registry::{Registry, CodecInfo, FormatInfo};
pub use filter::{Filter, FilterTarget, Params, ParamValue};
pub use output::{OutputConfig, VideoSettings, AudioSettings, Quality, Bitrate, EncodingSpeed};
pub use probe::{MediaMetadata, MediaProbe};
pub use ops::MediaOp;
pub use pipeline::{MediaPipeline, Progress};
pub use executor::MediaExecutor;
pub use subtitle::{SubtitleEntry, SubtitleStyle, SubtitlePosition, SubtitleTrack};
