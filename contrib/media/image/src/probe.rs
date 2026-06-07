//! Native image probe — inspect image metadata without FFmpeg.

use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;

use rskit_errors::{AppError, AppResult, ErrorCode};
use rskit_media::{
    codec::Codec,
    format::Format,
    probe::{MediaMetadata, MediaProbe},
    spatial::Resolution,
    time::Timestamp,
    track::{Track, VideoTrackInfo},
    types::{MediaType, TrackKind},
};
use rskit_storage::FileSource;

use crate::config::Config;
use crate::io;

/// Native image probe using the `image` crate.
///
/// Extracts resolution, format, and color type from images without
/// requiring FFmpeg. Faster than spawning an ffprobe process.
pub(crate) struct ImageProbe {
    config: Arc<Config>,
}

impl ImageProbe {
    /// Create a new image probe.
    pub(crate) fn new(config: Arc<Config>) -> Self {
        Self { config }
    }
}

impl Default for ImageProbe {
    fn default() -> Self {
        Self::new(Arc::new(Config::default()))
    }
}

#[async_trait::async_trait]
impl MediaProbe for ImageProbe {
    async fn probe(&self, source: &FileSource) -> AppResult<MediaMetadata> {
        let data = io::load_data(source, &self.config)?;
        let size = data.len() as u64;

        let decoded = io::decode_image(&data, &self.config)?;
        let format = decoded.format;
        let (format_name, codec_name) = match format {
            Some(image::ImageFormat::Jpeg) => ("jpeg", "mjpeg"),
            Some(image::ImageFormat::Png) => ("png", "png"),
            Some(image::ImageFormat::Gif) => ("gif", "gif"),
            Some(image::ImageFormat::Bmp) => ("bmp", "bmp"),
            Some(image::ImageFormat::Tiff) => ("tiff", "tiff"),
            Some(image::ImageFormat::WebP) => ("webp", "webp"),
            Some(image::ImageFormat::Avif) => ("avif", "av1"),
            _ => ("unknown", "unknown"),
        };

        let img = decoded.image;
        let (width, height) = (img.width(), img.height());

        let bit_depth = match img.color() {
            image::ColorType::L8
            | image::ColorType::La8
            | image::ColorType::Rgb8
            | image::ColorType::Rgba8 => Some(8u8),
            image::ColorType::L16
            | image::ColorType::La16
            | image::ColorType::Rgb16
            | image::ColorType::Rgba16 => Some(16),
            image::ColorType::Rgb32F | image::ColorType::Rgba32F => Some(32),
            _ => None,
        };

        let tracks = vec![Track {
            index: 0,
            kind: TrackKind::Video,
            codec: Some(Codec::new(codec_name)),
            bitrate: None,
            language: None,
            is_default: true,
            title: None,
            duration: None,
            video: Some(VideoTrackInfo {
                resolution: Resolution::new(width, height),
                frame_rate: None,
                pixel_format: None,
                rotation: None,
                color_space: None,
                color_range: None,
                bit_depth,
                profile: None,
                level: None,
                hdr: None,
            }),
            audio: None,
            subtitle: None,
        }];

        Ok(MediaMetadata {
            media_type: MediaType::Image,
            format: Format::new(format_name),
            duration: None,
            size: Some(size),
            bitrate: None,
            tracks,
            tags: HashMap::new(),
            created_at: None,
        })
    }

    async fn thumbnail(
        &self,
        source: &FileSource,
        _at: Timestamp,
        resolution: Option<Resolution>,
    ) -> AppResult<FileSource> {
        // For images, the thumbnail is just a resized version of the image itself
        let data = io::load_data(source, &self.config)?;
        let img = io::decode_image(&data, &self.config)?.image;

        let thumb = if let Some(res) = resolution {
            io::ensure_resolution(res, &self.config)?;
            img.resize(res.width, res.height, image::imageops::FilterType::Lanczos3)
        } else {
            // Default thumbnail: fit to 320px wide
            let ratio = 320.0 / img.width() as f64;
            let h = (img.height() as f64 * ratio) as u32;
            io::ensure_resolution(Resolution::new(320, h), &self.config)?;
            img.resize_exact(320, h, image::imageops::FilterType::Lanczos3)
        };
        io::ensure_image_dimensions(&thumb, &self.config)?;

        let mut buf = Vec::new();
        thumb
            .write_to(
                &mut std::io::Cursor::new(&mut buf),
                image::ImageFormat::Jpeg,
            )
            .map_err(|e| {
                AppError::new(
                    ErrorCode::Internal,
                    format!("failed to encode thumbnail: {e}"),
                )
            })?;

        Ok(FileSource::Bytes(bytes::Bytes::from(buf)))
    }

    async fn thumbnails(
        &self,
        source: &FileSource,
        _interval: Duration,
        resolution: Option<Resolution>,
    ) -> AppResult<Vec<FileSource>> {
        // For a single image, just return one thumbnail
        let thumb = self
            .thumbnail(source, Timestamp::from_millis(0), resolution)
            .await?;
        Ok(vec![thumb])
    }
}
