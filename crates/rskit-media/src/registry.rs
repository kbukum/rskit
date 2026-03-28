//! Data-driven codec & format registry.

use std::collections::HashMap;

use crate::{
    codec::{self, Codec, CodecKind},
    format::{self, Format},
    types::MediaType,
};

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
pub struct Registry {
    codecs: HashMap<Codec, CodecInfo>,
    formats: HashMap<Format, FormatInfo>,
}

impl Default for Registry {
    fn default() -> Self {
        let mut reg = Self {
            codecs: HashMap::new(),
            formats: HashMap::new(),
        };
        reg.load_defaults();
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

    /// Check if a codec is compatible with a format.
    pub fn is_compatible(&self, codec: &Codec, format: &Format) -> bool {
        self.codecs
            .get(codec)
            .is_some_and(|info| info.compatible_formats.contains(format))
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

    fn load_defaults(&mut self) {
        let mp4 = Format::new(format::MP4);
        let mkv = Format::new(format::MKV);
        let webm = Format::new(format::WEBM);
        let avi = Format::new(format::AVI);
        let mov = Format::new(format::MOV);
        let ts = Format::new(format::TS);
        let mp3f = Format::new(format::MP3);
        let wav = Format::new(format::WAV);
        let flacf = Format::new(format::FLAC);
        let ogg = Format::new(format::OGG);

        // Video codecs
        self.register_codec(CodecInfo {
            id: Codec::new(codec::video::H264),
            kind: CodecKind::Video,
            display_name: "H.264 / AVC".into(),
            ffmpeg_encoder: Some("libx264".into()),
            ffmpeg_decoder: Some("h264".into()),
            compatible_formats: vec![mp4.clone(), mkv.clone(), avi.clone(), mov.clone(), ts.clone()],
        });
        self.register_codec(CodecInfo {
            id: Codec::new(codec::video::H265),
            kind: CodecKind::Video,
            display_name: "H.265 / HEVC".into(),
            ffmpeg_encoder: Some("libx265".into()),
            ffmpeg_decoder: Some("hevc".into()),
            compatible_formats: vec![mp4.clone(), mkv.clone(), mov.clone(), ts.clone()],
        });
        self.register_codec(CodecInfo {
            id: Codec::new(codec::video::VP8),
            kind: CodecKind::Video,
            display_name: "VP8".into(),
            ffmpeg_encoder: Some("libvpx".into()),
            ffmpeg_decoder: Some("vp8".into()),
            compatible_formats: vec![mkv.clone(), webm.clone()],
        });
        self.register_codec(CodecInfo {
            id: Codec::new(codec::video::VP9),
            kind: CodecKind::Video,
            display_name: "VP9".into(),
            ffmpeg_encoder: Some("libvpx-vp9".into()),
            ffmpeg_decoder: Some("vp9".into()),
            compatible_formats: vec![mkv.clone(), webm.clone()],
        });
        self.register_codec(CodecInfo {
            id: Codec::new(codec::video::AV1),
            kind: CodecKind::Video,
            display_name: "AV1".into(),
            ffmpeg_encoder: Some("libaom-av1".into()),
            ffmpeg_decoder: Some("av1".into()),
            compatible_formats: vec![mp4.clone(), mkv.clone(), webm.clone()],
        });
        self.register_codec(CodecInfo {
            id: Codec::new(codec::video::PRORES),
            kind: CodecKind::Video,
            display_name: "Apple ProRes".into(),
            ffmpeg_encoder: Some("prores_ks".into()),
            ffmpeg_decoder: Some("prores".into()),
            compatible_formats: vec![mov.clone()],
        });
        self.register_codec(CodecInfo {
            id: Codec::new(codec::video::MPEG2),
            kind: CodecKind::Video,
            display_name: "MPEG-2".into(),
            ffmpeg_encoder: Some("mpeg2video".into()),
            ffmpeg_decoder: Some("mpeg2video".into()),
            compatible_formats: vec![avi.clone(), ts.clone()],
        });
        self.register_codec(CodecInfo {
            id: Codec::new(codec::video::MPEG4),
            kind: CodecKind::Video,
            display_name: "MPEG-4 Part 2".into(),
            ffmpeg_encoder: Some("mpeg4".into()),
            ffmpeg_decoder: Some("mpeg4".into()),
            compatible_formats: vec![avi.clone()],
        });
        self.register_codec(CodecInfo {
            id: Codec::new(codec::video::THEORA),
            kind: CodecKind::Video,
            display_name: "Theora".into(),
            ffmpeg_encoder: Some("libtheora".into()),
            ffmpeg_decoder: Some("theora".into()),
            compatible_formats: vec![ogg.clone()],
        });

        // Audio codecs
        self.register_codec(CodecInfo {
            id: Codec::new(codec::audio::AAC),
            kind: CodecKind::Audio,
            display_name: "AAC".into(),
            ffmpeg_encoder: Some("aac".into()),
            ffmpeg_decoder: Some("aac".into()),
            compatible_formats: vec![mp4.clone(), mkv.clone(), mov.clone(), ts.clone()],
        });
        self.register_codec(CodecInfo {
            id: Codec::new(codec::audio::OPUS),
            kind: CodecKind::Audio,
            display_name: "Opus".into(),
            ffmpeg_encoder: Some("libopus".into()),
            ffmpeg_decoder: Some("opus".into()),
            compatible_formats: vec![mp4.clone(), mkv.clone(), webm.clone(), ogg.clone()],
        });
        self.register_codec(CodecInfo {
            id: Codec::new(codec::audio::MP3),
            kind: CodecKind::Audio,
            display_name: "MP3".into(),
            ffmpeg_encoder: Some("libmp3lame".into()),
            ffmpeg_decoder: Some("mp3".into()),
            compatible_formats: vec![mp4.clone(), mkv.clone(), avi.clone(), ts.clone(), mp3f.clone()],
        });
        self.register_codec(CodecInfo {
            id: Codec::new(codec::audio::FLAC),
            kind: CodecKind::Audio,
            display_name: "FLAC".into(),
            ffmpeg_encoder: Some("flac".into()),
            ffmpeg_decoder: Some("flac".into()),
            compatible_formats: vec![mkv.clone(), ogg.clone(), flacf.clone()],
        });
        self.register_codec(CodecInfo {
            id: Codec::new(codec::audio::VORBIS),
            kind: CodecKind::Audio,
            display_name: "Vorbis".into(),
            ffmpeg_encoder: Some("libvorbis".into()),
            ffmpeg_decoder: Some("vorbis".into()),
            compatible_formats: vec![mkv.clone(), webm.clone(), ogg.clone()],
        });
        self.register_codec(CodecInfo {
            id: Codec::new(codec::audio::PCM),
            kind: CodecKind::Audio,
            display_name: "PCM".into(),
            ffmpeg_encoder: Some("pcm_s16le".into()),
            ffmpeg_decoder: Some("pcm_s16le".into()),
            compatible_formats: vec![avi.clone(), mov.clone(), wav.clone()],
        });
        self.register_codec(CodecInfo {
            id: Codec::new(codec::audio::AC3),
            kind: CodecKind::Audio,
            display_name: "Dolby Digital".into(),
            ffmpeg_encoder: Some("ac3".into()),
            ffmpeg_decoder: Some("ac3".into()),
            compatible_formats: vec![mp4.clone(), avi.clone(), ts.clone()],
        });
        self.register_codec(CodecInfo {
            id: Codec::new(codec::audio::EAC3),
            kind: CodecKind::Audio,
            display_name: "Dolby Digital Plus".into(),
            ffmpeg_encoder: Some("eac3".into()),
            ffmpeg_decoder: Some("eac3".into()),
            compatible_formats: vec![mp4.clone()],
        });
        self.register_codec(CodecInfo {
            id: Codec::new(codec::audio::ALAC),
            kind: CodecKind::Audio,
            display_name: "Apple Lossless".into(),
            ffmpeg_encoder: Some("alac".into()),
            ffmpeg_decoder: Some("alac".into()),
            compatible_formats: vec![mov.clone()],
        });

        // Video container formats
        self.register_format(FormatInfo {
            id: mp4.clone(),
            extension: "mp4".into(),
            mime_type: "video/mp4".into(),
            is_container: true,
            supported_media_types: vec![MediaType::Video, MediaType::Audio],
            default_video_codec: Some(Codec::new(codec::video::H264)),
            default_audio_codec: Some(Codec::new(codec::audio::AAC)),
        });
        self.register_format(FormatInfo {
            id: mkv.clone(),
            extension: "mkv".into(),
            mime_type: "video/x-matroska".into(),
            is_container: true,
            supported_media_types: vec![MediaType::Video, MediaType::Audio],
            default_video_codec: Some(Codec::new(codec::video::H264)),
            default_audio_codec: Some(Codec::new(codec::audio::AAC)),
        });
        self.register_format(FormatInfo {
            id: webm.clone(),
            extension: "webm".into(),
            mime_type: "video/webm".into(),
            is_container: true,
            supported_media_types: vec![MediaType::Video, MediaType::Audio],
            default_video_codec: Some(Codec::new(codec::video::VP9)),
            default_audio_codec: Some(Codec::new(codec::audio::OPUS)),
        });
        self.register_format(FormatInfo {
            id: avi.clone(),
            extension: "avi".into(),
            mime_type: "video/x-msvideo".into(),
            is_container: true,
            supported_media_types: vec![MediaType::Video, MediaType::Audio],
            default_video_codec: Some(Codec::new(codec::video::H264)),
            default_audio_codec: Some(Codec::new(codec::audio::MP3)),
        });
        self.register_format(FormatInfo {
            id: mov.clone(),
            extension: "mov".into(),
            mime_type: "video/quicktime".into(),
            is_container: true,
            supported_media_types: vec![MediaType::Video, MediaType::Audio],
            default_video_codec: Some(Codec::new(codec::video::H264)),
            default_audio_codec: Some(Codec::new(codec::audio::AAC)),
        });
        self.register_format(FormatInfo {
            id: ts.clone(),
            extension: "ts".into(),
            mime_type: "video/mp2t".into(),
            is_container: true,
            supported_media_types: vec![MediaType::Video, MediaType::Audio],
            default_video_codec: Some(Codec::new(codec::video::H264)),
            default_audio_codec: Some(Codec::new(codec::audio::AAC)),
        });

        // Audio-only formats
        self.register_format(FormatInfo {
            id: mp3f.clone(),
            extension: "mp3".into(),
            mime_type: "audio/mpeg".into(),
            is_container: false,
            supported_media_types: vec![MediaType::Audio],
            default_video_codec: None,
            default_audio_codec: Some(Codec::new(codec::audio::MP3)),
        });
        self.register_format(FormatInfo {
            id: wav.clone(),
            extension: "wav".into(),
            mime_type: "audio/wav".into(),
            is_container: false,
            supported_media_types: vec![MediaType::Audio],
            default_video_codec: None,
            default_audio_codec: Some(Codec::new(codec::audio::PCM)),
        });
        self.register_format(FormatInfo {
            id: flacf.clone(),
            extension: "flac".into(),
            mime_type: "audio/flac".into(),
            is_container: false,
            supported_media_types: vec![MediaType::Audio],
            default_video_codec: None,
            default_audio_codec: Some(Codec::new(codec::audio::FLAC)),
        });
        self.register_format(FormatInfo {
            id: ogg.clone(),
            extension: "ogg".into(),
            mime_type: "audio/ogg".into(),
            is_container: true,
            supported_media_types: vec![MediaType::Video, MediaType::Audio],
            default_video_codec: Some(Codec::new(codec::video::THEORA)),
            default_audio_codec: Some(Codec::new(codec::audio::VORBIS)),
        });

        // Image formats
        for (id, ext, mime) in [
            (format::PNG, "png", "image/png"),
            (format::JPEG, "jpeg", "image/jpeg"),
            (format::WEBP, "webp", "image/webp"),
            (format::GIF, "gif", "image/gif"),
            (format::BMP, "bmp", "image/bmp"),
            (format::TIFF, "tiff", "image/tiff"),
            (format::SVG, "svg", "image/svg+xml"),
            (format::AVIF, "avif", "image/avif"),
            (format::HEIF, "heif", "image/heif"),
        ] {
            self.register_format(FormatInfo {
                id: Format::new(id),
                extension: ext.into(),
                mime_type: mime.into(),
                is_container: false,
                supported_media_types: vec![MediaType::Image],
                default_video_codec: None,
                default_audio_codec: None,
            });
        }
    }
}
