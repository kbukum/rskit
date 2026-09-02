//! Core media type enums.

use serde::{Deserialize, Serialize};

/// The broad media category.
///
/// Serializes as a stable lowercase string (`video`/`audio`/`image`/`text`/`unknown`),
/// never an integer, so the wire form is cross-kit compatible. Unrecognized values
/// decode to [`MediaType::Unknown`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
#[non_exhaustive]
pub enum MediaType {
    /// Video content.
    Video,
    /// Audio content.
    Audio,
    /// Image content.
    Image,
    /// Text content detected via a printable-ratio heuristic.
    Text,
    /// Content that could not be classified.
    #[serde(other)]
    Unknown,
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

#[cfg(test)]
mod tests {
    use super::MediaType;

    #[test]
    fn media_type_serializes_as_lowercase_strings() {
        let cases = [
            (MediaType::Video, "\"video\""),
            (MediaType::Audio, "\"audio\""),
            (MediaType::Image, "\"image\""),
            (MediaType::Text, "\"text\""),
            (MediaType::Unknown, "\"unknown\""),
        ];
        for (variant, expected) in cases {
            let encoded = serde_json::to_string(&variant).expect("serialize media type");
            assert_eq!(encoded, expected);
            let decoded: MediaType =
                serde_json::from_str(expected).expect("deserialize media type");
            assert_eq!(decoded, variant);
        }
    }

    #[test]
    fn media_type_decodes_unknown_values_to_unknown() {
        let decoded: MediaType =
            serde_json::from_str("\"hologram\"").expect("deserialize unknown media type");
        assert_eq!(decoded, MediaType::Unknown);
    }
}
