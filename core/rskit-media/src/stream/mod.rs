//! Stream metadata vocabulary — sample rate/channel layout, resolution/frame
//! rate, timestamps, track info, subtitles, and core media type enumerations.

/// Audio sample rate and channel layout types.
pub mod audio;
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
