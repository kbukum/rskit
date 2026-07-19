//! Data-driven codec & format registry.

use std::collections::HashMap;
use std::sync::Arc;

use rskit_errors::{AppError, AppResult, ErrorCode};

use crate::{
    codec::{Codec, CodecKind},
    executor::MediaExecutor,
    format::Format,
    probe::MediaProbe,
    types::MediaType,
};

use super::defaults;

/// Factory function used to build a configured media executor.
pub type ExecutorFactory = Arc<dyn Fn() -> AppResult<Arc<dyn MediaExecutor>> + Send + Sync>;

/// Factory function used to build a configured media probe.
pub type ProbeFactory = Arc<dyn Fn() -> AppResult<Arc<dyn MediaProbe>> + Send + Sync>;

/// Metadata about a codec.
#[derive(Debug, Clone)]
pub struct CodecInfo {
    /// Codec identifier.
    pub id: Codec,
    /// What domain this codec belongs to.
    pub kind: CodecKind,
    /// Human-readable name.
    pub display_name: String,
    /// FFmpeg encoder name (e.g., "libx264").
    pub ffmpeg_encoder: Option<String>,
    /// FFmpeg decoder name.
    pub ffmpeg_decoder: Option<String>,
    /// Compatible container formats.
    pub compatible_formats: Vec<Format>,
}

/// Metadata about a container format.
#[derive(Debug, Clone)]
pub struct FormatInfo {
    /// Format identifier.
    pub id: Format,
    /// Default file extension (e.g., "mp4").
    pub extension: String,
    /// MIME type (e.g., "video/mp4").
    pub mime_type: String,
    /// Whether this format is a multi-track container.
    pub is_container: bool,
    /// What media types this format can hold.
    pub supported_media_types: Vec<MediaType>,
    /// Default video codec for this format.
    pub default_video_codec: Option<Codec>,
    /// Default audio codec for this format.
    pub default_audio_codec: Option<Codec>,
}

/// Central knowledge base for codec/format information and compatibility.
#[derive(Clone)]
pub struct Registry {
    codecs: HashMap<Codec, CodecInfo>,
    formats: HashMap<Format, FormatInfo>,
    executor_factories: HashMap<String, ExecutorFactory>,
    probe_factories: HashMap<String, ProbeFactory>,
}

impl Default for Registry {
    fn default() -> Self {
        let mut reg = Self {
            codecs: HashMap::new(),
            formats: HashMap::new(),
            executor_factories: HashMap::new(),
            probe_factories: HashMap::new(),
        };
        defaults::load_defaults(&mut reg);
        reg
    }
}

impl Registry {
    /// Register a custom codec.
    pub fn register_codec(&mut self, info: CodecInfo) {
        self.codecs.insert(info.id.clone(), info);
    }

    /// Register a custom format.
    pub fn register_format(&mut self, info: FormatInfo) {
        self.formats.insert(info.id.clone(), info);
    }

    /// Register a configured media executor factory.
    pub fn register_executor(
        &mut self,
        name: impl Into<String>,
        factory: ExecutorFactory,
    ) -> AppResult<()> {
        let name = Self::normalize_backend_name(name)?;
        if self.executor_factories.contains_key(&name) {
            return Err(AppError::new(
                ErrorCode::AlreadyExists,
                format!("media executor '{name}' is already registered"),
            ));
        }
        self.executor_factories.insert(name, factory);
        Ok(())
    }

    /// Register a configured media probe factory.
    pub fn register_probe(
        &mut self,
        name: impl Into<String>,
        factory: ProbeFactory,
    ) -> AppResult<()> {
        let name = Self::normalize_backend_name(name)?;
        if self.probe_factories.contains_key(&name) {
            return Err(AppError::new(
                ErrorCode::AlreadyExists,
                format!("media probe '{name}' is already registered"),
            ));
        }
        self.probe_factories.insert(name, factory);
        Ok(())
    }

    /// Build the configured executor for `name`.
    pub fn executor(&self, name: &str) -> AppResult<Arc<dyn MediaExecutor>> {
        let name = Self::normalize_backend_name(name)?;
        self.executor_factories.get(&name).ok_or_else(|| {
            AppError::new(
                ErrorCode::NotFound,
                format!("media executor '{name}' is not registered"),
            )
        })?()
    }

    /// Build the configured probe for `name`.
    pub fn probe(&self, name: &str) -> AppResult<Arc<dyn MediaProbe>> {
        let name = Self::normalize_backend_name(name)?;
        self.probe_factories.get(&name).ok_or_else(|| {
            AppError::new(
                ErrorCode::NotFound,
                format!("media probe '{name}' is not registered"),
            )
        })?()
    }

    /// Check if a codec is compatible with a format.
    pub fn is_compatible(&self, codec: &Codec, format: &Format) -> bool {
        self.codecs
            .get(codec)
            .is_some_and(|info| info.compatible_formats.contains(format))
    }

    fn normalize_backend_name(name: impl Into<String>) -> AppResult<String> {
        let name = name.into().trim().to_owned();
        if name.is_empty() {
            return Err(AppError::new(
                ErrorCode::InvalidInput,
                "media backend name is required",
            ));
        }
        Ok(name)
    }

    /// Get the default codec pair (video, audio) for a format.
    pub fn default_codecs(&self, format: &Format) -> Option<(Codec, Codec)> {
        let info = self.formats.get(format)?;
        let video = info.default_video_codec.clone()?;
        let audio = info.default_audio_codec.clone()?;
        Some((video, audio))
    }

    /// Look up codec metadata.
    pub fn codec_info(&self, codec: &Codec) -> Option<&CodecInfo> {
        self.codecs.get(codec)
    }

    /// Look up format metadata.
    pub fn format_info(&self, format: &Format) -> Option<&FormatInfo> {
        self.formats.get(format)
    }

    /// Detect format from a file extension.
    pub fn format_from_extension(&self, ext: &str) -> Option<&FormatInfo> {
        let ext_lower = ext.to_lowercase();
        self.formats.values().find(|f| f.extension == ext_lower)
    }

    /// Detect format from a MIME type.
    pub fn format_from_mime(&self, mime: &str) -> Option<&FormatInfo> {
        self.formats.values().find(|f| f.mime_type == mime)
    }

    /// List all registered codecs of a given kind.
    pub fn codecs_by_kind(&self, kind: CodecKind) -> Vec<&CodecInfo> {
        self.codecs.values().filter(|c| c.kind == kind).collect()
    }

    /// List all formats compatible with a given codec.
    pub fn formats_for_codec(&self, codec: &Codec) -> Vec<&FormatInfo> {
        match self.codecs.get(codec) {
            Some(info) => info
                .compatible_formats
                .iter()
                .filter_map(|f| self.formats.get(f))
                .collect(),
            None => Vec::new(),
        }
    }
}

#[cfg(test)]
mod tests {
    use rskit_errors::ErrorCode;

    use super::Registry;

    #[test]
    fn executor_rejects_empty_backend_name() {
        let Err(err) = Registry::default().executor(" \t ") else {
            panic!("empty executor name should be rejected");
        };
        assert_eq!(err.code(), ErrorCode::InvalidInput);
    }

    #[test]
    fn probe_rejects_empty_backend_name() {
        let Err(err) = Registry::default().probe(" \t ") else {
            panic!("empty probe name should be rejected");
        };
        assert_eq!(err.code(), ErrorCode::InvalidInput);
    }
}
