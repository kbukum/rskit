//! FFmpeg probe implementation.

use std::collections::HashMap;
use std::time::Duration;

use rskit_errors::{AppError, AppResult, ErrorCode};
use rskit_file::FileSource;
use rskit_media::{
    audio::{ChannelLayout, SampleRate},
    codec::Codec,
    format::Format,
    probe::{MediaMetadata, MediaProbe},
    spatial::{FrameRate, Resolution},
    time::Timestamp,
    track::{AudioTrackInfo, SubtitleTrackInfo, Track, VideoTrackInfo},
    types::{MediaType, TrackKind},
};

use crate::config::FfmpegConfig;

/// FFmpeg-based media probe using `ffprobe`.
pub struct FfmpegProbe {
    config: FfmpegConfig,
}

impl FfmpegProbe {
    /// Create a new probe with the given configuration.
    pub fn new(config: FfmpegConfig) -> Self {
        Self { config }
    }

    /// Check that ffprobe is available and return its version.
    pub async fn check_available(&self) -> AppResult<String> {
        let output = tokio::process::Command::new(self.config.ffprobe_bin())
            .arg("-version")
            .output()
            .await
            .map_err(|e| {
                AppError::new(ErrorCode::ServiceUnavailable, format!("ffprobe not found: {e}"))
            })?;

        let stdout = String::from_utf8_lossy(&output.stdout);
        let version = stdout.lines().next().unwrap_or("unknown").to_string();
        Ok(version)
    }

    /// Run ffprobe and return the raw JSON output.
    pub async fn probe_raw(&self, source: &FileSource) -> AppResult<serde_json::Value> {
        let resolved = source.to_local_path().await?;
        let path = resolved.path();

        let output = tokio::process::Command::new(self.config.ffprobe_bin())
            .args([
                "-v", "quiet",
                "-print_format", "json",
                "-show_format",
                "-show_streams",
            ])
            .arg(path)
            .output()
            .await
            .map_err(|e| {
                AppError::new(ErrorCode::Internal, format!("ffprobe execution failed: {e}"))
            })?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            return Err(AppError::new(
                ErrorCode::Internal,
                format!("ffprobe failed: {stderr}"),
            ));
        }

        let json: serde_json::Value =
            serde_json::from_slice(&output.stdout).map_err(|e| {
                AppError::new(ErrorCode::Internal, format!("ffprobe output is not valid JSON: {e}"))
            })?;

        Ok(json)
    }

    fn parse_metadata(json: &serde_json::Value) -> AppResult<MediaMetadata> {
        let format_obj = json.get("format").unwrap_or(&serde_json::Value::Null);
        let streams = json
            .get("streams")
            .and_then(|s| s.as_array())
            .cloned()
            .unwrap_or_default();

        let format_name = format_obj
            .get("format_name")
            .and_then(|v| v.as_str())
            .unwrap_or("unknown");

        let duration = format_obj
            .get("duration")
            .and_then(|v| v.as_str())
            .and_then(|s| s.parse::<f64>().ok())
            .map(|s| Duration::from_secs_f64(s));

        let size = format_obj
            .get("size")
            .and_then(|v| v.as_str())
            .and_then(|s| s.parse::<u64>().ok());

        let bitrate = format_obj
            .get("bit_rate")
            .and_then(|v| v.as_str())
            .and_then(|s| s.parse::<u64>().ok());

        let tags: HashMap<String, String> = format_obj
            .get("tags")
            .and_then(|t| t.as_object())
            .map(|obj| {
                obj.iter()
                    .map(|(k, v)| (k.clone(), v.as_str().unwrap_or("").to_string()))
                    .collect()
            })
            .unwrap_or_default();

        let mut tracks = Vec::new();
        let mut has_video = false;
        let mut has_audio = false;

        for (i, stream) in streams.iter().enumerate() {
            let codec_type = stream
                .get("codec_type")
                .and_then(|v| v.as_str())
                .unwrap_or("");

            let kind = match codec_type {
                "video" => {
                    has_video = true;
                    TrackKind::Video
                }
                "audio" => {
                    has_audio = true;
                    TrackKind::Audio
                }
                "subtitle" => TrackKind::Subtitle,
                "data" => TrackKind::Data,
                "attachment" => TrackKind::Attachment,
                _ => continue,
            };

            let codec_name = stream
                .get("codec_name")
                .and_then(|v| v.as_str())
                .map(|s| Codec::new(s));

            let bit_rate = stream
                .get("bit_rate")
                .and_then(|v| v.as_str())
                .and_then(|s| s.parse::<u64>().ok());

            let language = stream
                .get("tags")
                .and_then(|t| t.get("language"))
                .and_then(|v| v.as_str())
                .map(String::from);

            let track_duration = stream
                .get("duration")
                .and_then(|v| v.as_str())
                .and_then(|s| s.parse::<f64>().ok())
                .map(Duration::from_secs_f64);

            let video = if kind == TrackKind::Video {
                let width = stream.get("width").and_then(|v| v.as_u64()).unwrap_or(0) as u32;
                let height = stream.get("height").and_then(|v| v.as_u64()).unwrap_or(0) as u32;

                let frame_rate = stream
                    .get("r_frame_rate")
                    .and_then(|v| v.as_str())
                    .and_then(|s| {
                        let parts: Vec<&str> = s.split('/').collect();
                        if parts.len() == 2 {
                            let num = parts[0].parse::<u32>().ok()?;
                            let den = parts[1].parse::<u32>().ok()?;
                            if den > 0 { Some(FrameRate::new(num, den)) } else { None }
                        } else {
                            None
                        }
                    });

                let pix_fmt = stream
                    .get("pix_fmt")
                    .and_then(|v| v.as_str())
                    .map(String::from);

                let rotation = stream
                    .get("tags")
                    .and_then(|t| t.get("rotate"))
                    .and_then(|v| v.as_str())
                    .and_then(|s| s.parse::<i16>().ok());

                Some(VideoTrackInfo {
                    resolution: Resolution::new(width, height),
                    frame_rate,
                    pixel_format: pix_fmt,
                    rotation,
                    color_space: None,
                    bit_depth: None,
                    hdr: false,
                })
            } else {
                None
            };

            let audio = if kind == TrackKind::Audio {
                let sample_rate = stream
                    .get("sample_rate")
                    .and_then(|v| v.as_str())
                    .and_then(|s| s.parse::<u32>().ok())
                    .unwrap_or(44100);

                let channels = stream
                    .get("channels")
                    .and_then(|v| v.as_u64())
                    .unwrap_or(2) as u16;

                let layout = match channels {
                    1 => ChannelLayout::Mono,
                    2 => ChannelLayout::Stereo,
                    6 => ChannelLayout::Surround51,
                    8 => ChannelLayout::Surround71,
                    n => ChannelLayout::Custom(n),
                };

                Some(AudioTrackInfo {
                    sample_rate: SampleRate::hz(sample_rate),
                    channels: layout,
                    bit_depth: None,
                })
            } else {
                None
            };

            let subtitle = if kind == TrackKind::Subtitle {
                let fmt = stream
                    .get("codec_name")
                    .and_then(|v| v.as_str())
                    .unwrap_or("unknown")
                    .to_string();

                Some(SubtitleTrackInfo {
                    format: fmt,
                    forced: false,
                })
            } else {
                None
            };

            let is_default = stream
                .get("disposition")
                .and_then(|d| d.get("default"))
                .and_then(|v| v.as_u64())
                .unwrap_or(0) == 1;

            tracks.push(Track {
                index: i,
                kind,
                codec: codec_name,
                bitrate: bit_rate,
                language,
                is_default,
                title: None,
                duration: track_duration,
                video,
                audio,
                subtitle,
            });
        }

        let media_type = if has_video {
            MediaType::Video
        } else if has_audio {
            MediaType::Audio
        } else {
            MediaType::Image
        };

        // Map common format names to our format IDs
        let format_id = match format_name.split(',').next().unwrap_or("") {
            "mov" | "mp4" | "m4a" | "3gp" | "3g2" | "mj2" => "mp4",
            "matroska" | "webm" => {
                if format_name.contains("webm") {
                    "webm"
                } else {
                    "mkv"
                }
            }
            "avi" => "avi",
            "mpegts" => "ts",
            "flv" => "flv",
            "wav" => "wav",
            "mp3" => "mp3",
            "flac" => "flac",
            "ogg" => "ogg",
            other => other,
        };

        Ok(MediaMetadata {
            media_type,
            format: Format::new(format_id),
            duration,
            size,
            bitrate,
            tracks,
            tags,
            created_at: None,
        })
    }
}

#[async_trait::async_trait]
impl MediaProbe for FfmpegProbe {
    async fn probe(&self, source: &FileSource) -> AppResult<MediaMetadata> {
        let json = self.probe_raw(source).await?;
        Self::parse_metadata(&json)
    }

    async fn thumbnail(
        &self,
        source: &FileSource,
        at: Timestamp,
        resolution: Option<Resolution>,
    ) -> AppResult<FileSource> {
        let resolved = source.to_local_path().await?;
        let tmp = rskit_file::TempFile::with_extension("jpg")?;

        let mut args = vec![
            "-ss".to_string(),
            at.to_ffmpeg_time(),
            "-i".to_string(),
            resolved.path().to_string_lossy().to_string(),
            "-vframes".to_string(),
            "1".to_string(),
        ];

        if let Some(res) = resolution {
            args.extend([
                "-vf".to_string(),
                format!("scale={}:{}", res.width, res.height),
            ]);
        }

        args.extend(["-y".to_string(), tmp.path().to_string_lossy().to_string()]);

        let output = tokio::process::Command::new(self.config.ffmpeg_bin())
            .args(&args)
            .output()
            .await
            .map_err(|e| {
                AppError::new(ErrorCode::Internal, format!("ffmpeg thumbnail failed: {e}"))
            })?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            return Err(AppError::new(
                ErrorCode::Internal,
                format!("ffmpeg thumbnail failed: {stderr}"),
            ));
        }

        Ok(tmp.into_source())
    }

    async fn thumbnails(
        &self,
        source: &FileSource,
        interval: Duration,
        resolution: Option<Resolution>,
    ) -> AppResult<Vec<FileSource>> {
        let resolved = source.to_local_path().await?;
        let tmp_dir = rskit_file::TempDir::new()?;
        let pattern = tmp_dir.path().join("thumb_%04d.jpg");

        let mut vf = format!("fps=1/{}", interval.as_secs().max(1));
        if let Some(res) = resolution {
            vf.push_str(&format!(",scale={}:{}", res.width, res.height));
        }

        let output = tokio::process::Command::new(self.config.ffmpeg_bin())
            .args([
                "-i",
                &resolved.path().to_string_lossy(),
                "-vf",
                &vf,
                "-y",
                &pattern.to_string_lossy(),
            ])
            .output()
            .await
            .map_err(|e| {
                AppError::new(ErrorCode::Internal, format!("ffmpeg thumbnails failed: {e}"))
            })?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            return Err(AppError::new(
                ErrorCode::Internal,
                format!("ffmpeg thumbnails failed: {stderr}"),
            ));
        }

        // Collect generated thumbnails
        let mut results = Vec::new();
        let mut entries = tokio::fs::read_dir(tmp_dir.path()).await.map_err(|e| {
            AppError::new(ErrorCode::Internal, format!("failed to read thumb dir: {e}"))
        })?;

        let mut paths = Vec::new();
        while let Some(entry) = entries.next_entry().await.map_err(|e| {
            AppError::new(ErrorCode::Internal, format!("failed to read entry: {e}"))
        })? {
            let p = entry.path();
            if p.extension().is_some_and(|e| e == "jpg") {
                paths.push(p);
            }
        }
        paths.sort();

        for p in paths {
            let data = tokio::fs::read(&p).await.map_err(|e| {
                AppError::new(ErrorCode::Internal, format!("failed to read thumb: {e}"))
            })?;
            results.push(FileSource::Bytes(bytes::Bytes::from(data)));
        }

        Ok(results)
    }
}
