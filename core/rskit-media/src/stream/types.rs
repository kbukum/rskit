//! Core media type enums.

use serde::{Deserialize, Serialize};

/// The broad media category.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum MediaType {
    /// Video content.
    Video,
    /// Audio content.
    Audio,
    /// Image content.
    Image,
}

/// Kind of track in a media container.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum TrackKind {
    /// Video track.
    Video,
    /// Audio track.
    Audio,
    /// Subtitle track.
    Subtitle,
    /// Data track (e.g., chapter markers).
    Data,
    /// Attachment (e.g., fonts, thumbnails).
    Attachment,
}
