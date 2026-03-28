//! Dataset collection framework — source, transform, target, collector.

pub mod collector;
pub mod manifest;
pub mod source;
pub mod target;
pub mod transform;

use serde::{Deserialize, Serialize};

pub use collector::{Collector, CollectorConfig, CollectorResult, NullProgress, ProgressCallback};
pub use manifest::Manifest;
pub use source::Source;
pub use target::{PublishResult, Target};
pub use transform::Transform;

/// Binary classification label.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[repr(u8)]
pub enum Label {
    Real = 0,
    AiGenerated = 1,
}

impl std::fmt::Display for Label {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Label::Real => write!(f, "real"),
            Label::AiGenerated => write!(f, "ai"),
        }
    }
}

/// Supported media types.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum MediaType {
    Image,
    Text,
    Audio,
    Video,
}

impl std::fmt::Display for MediaType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            MediaType::Image => write!(f, "image"),
            MediaType::Text => write!(f, "text"),
            MediaType::Audio => write!(f, "audio"),
            MediaType::Video => write!(f, "video"),
        }
    }
}

/// A single data sample flowing through the pipeline.
#[derive(Debug, Clone)]
pub struct DataItem {
    pub content: Vec<u8>,
    pub label: Label,
    pub media_type: MediaType,
    pub source_name: String,
    pub extension: String,
    pub metadata: std::collections::HashMap<String, String>,
}

impl DataItem {
    pub fn new(
        content: Vec<u8>,
        label: Label,
        media_type: MediaType,
        source_name: impl Into<String>,
    ) -> Self {
        Self {
            content,
            label,
            media_type,
            source_name: source_name.into(),
            extension: ".jpg".to_string(),
            metadata: std::collections::HashMap::new(),
        }
    }

    pub fn with_extension(mut self, ext: impl Into<String>) -> Self {
        self.extension = ext.into();
        self
    }

    pub fn with_metadata(mut self, key: impl Into<String>, value: impl Into<String>) -> Self {
        self.metadata.insert(key.into(), value.into());
        self
    }
}
