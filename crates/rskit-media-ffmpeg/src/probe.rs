//! FFmpeg probe implementation.

use std::collections::HashMap;
use std::time::Duration;

use rskit_errors::{AppError, AppResult, ErrorCode};
use rskit_file::FileSource;
use rskit_media::{
    audio::{ChannelLayout, SampleRate},
    codec::Codec,
    color::{ColorRange, ColorSpace, PixelFormat},
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
                AppError::new(
                    ErrorCode::ServiceUnavailable,
                    format!("ffprobe not found: {e}"),
                )
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
                "-v",
                "quiet",
                "-print_format",
                "json",
                "-show_format",
                "-show_streams",
                "-show_chapters",
            ])
            .arg(path)
            .output()
            .await
            .map_err(|e| {
                AppError::new(
                    ErrorCode::Internal,
                    format!("ffprobe execution failed: {e}"),
                )
            })?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            return Err(AppError::new(
                ErrorCode::Internal,
                format!("ffprobe failed: {stderr}"),
            ));
        }

        let json: serde_json::Value = serde_json::from_slice(&output.stdout).map_err(|e| {
            AppError::new(
                ErrorCode::Internal,
                format!("ffprobe output is not valid JSON: {e}"),
            )
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
            .map(Duration::from_secs_f64);

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
                .map(Codec::new);

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
                            if den > 0 {
                                Some(FrameRate::new(num, den))
                            } else {
                                None
                            }
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

                // Parse color space from ffprobe's color_space field
                let color_space = stream
                    .get("color_space")
                    .and_then(|v| v.as_str())
                    .map(ColorSpace::from_ffmpeg)
                    .filter(|cs| *cs != ColorSpace::Unknown);

                // Parse color range (tv=Limited, pc=Full)
                let color_range =
                    stream
                        .get("color_range")
                        .and_then(|v| v.as_str())
                        .map(|r| match r {
                            "pc" | "jpeg" | "full" => ColorRange::Full,
                            _ => ColorRange::Limited,
                        });

                // Parse bit depth from bits_per_raw_sample
                let bit_depth = stream
                    .get("bits_per_raw_sample")
                    .and_then(|v| v.as_str())
                    .and_then(|s| s.parse::<u8>().ok())
                    .or_else(|| {
                        // Infer from pixel format name
                        pix_fmt.as_deref().and_then(|fmt| {
                            if fmt.contains("10le") || fmt.contains("10be") || fmt.ends_with("p10")
                            {
                                Some(10)
                            } else if fmt.contains("12le")
                                || fmt.contains("12be")
                                || fmt.ends_with("p12")
                            {
                                Some(12)
                            } else {
                                Some(8)
                            }
                        })
                    });

                // Parse codec profile from ffprobe
                let profile = stream
                    .get("profile")
                    .and_then(|v| v.as_str())
                    .and_then(rskit_media::CodecProfile::from_ffprobe);

                Some(VideoTrackInfo {
                    resolution: Resolution::new(width, height),
                    frame_rate,
                    pixel_format: pix_fmt.map(PixelFormat::new),
                    rotation,
                    color_space,
                    color_range,
                    bit_depth,
                    profile,
                    level: None,
                    hdr: None,
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

                let channels = stream.get("channels").and_then(|v| v.as_u64()).unwrap_or(2) as u16;

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

                let forced = stream
                    .get("disposition")
                    .and_then(|d| d.get("forced"))
                    .and_then(|v| v.as_u64())
                    .unwrap_or(0)
                    == 1;

                Some(SubtitleTrackInfo {
                    format: fmt,
                    forced,
                })
            } else {
                None
            };

            let is_default = stream
                .get("disposition")
                .and_then(|d| d.get("default"))
                .and_then(|v| v.as_u64())
                .unwrap_or(0)
                == 1;

            let title = stream
                .get("tags")
                .and_then(|t| t.get("title"))
                .and_then(|v| v.as_str())
                .map(String::from);

            tracks.push(Track {
                index: i,
                kind,
                codec: codec_name,
                bitrate: bit_rate,
                language,
                is_default,
                title,
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
                AppError::new(
                    ErrorCode::Internal,
                    format!("ffmpeg thumbnails failed: {e}"),
                )
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
            AppError::new(
                ErrorCode::Internal,
                format!("failed to read thumb dir: {e}"),
            )
        })?;

        let mut paths = Vec::new();
        while let Some(entry) = entries
            .next_entry()
            .await
            .map_err(|e| AppError::new(ErrorCode::Internal, format!("failed to read entry: {e}")))?
        {
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

    async fn sprite_sheet(
        &self,
        source: &FileSource,
        interval: Duration,
        thumb_resolution: Resolution,
        columns: u32,
    ) -> AppResult<FileSource> {
        let resolved = source.to_local_path().await?;
        let tmp = rskit_file::TempFile::with_extension("jpg")?;

        let vf = format!(
            "fps=1/{},scale={}:{},tile={}x0",
            interval.as_secs().max(1),
            thumb_resolution.width,
            thumb_resolution.height,
            columns,
        );

        let output = tokio::process::Command::new(self.config.ffmpeg_bin())
            .args([
                "-i",
                &resolved.path().to_string_lossy(),
                "-vf",
                &vf,
                "-frames:v",
                "1",
                "-y",
                &tmp.path().to_string_lossy(),
            ])
            .output()
            .await
            .map_err(|e| {
                AppError::new(
                    ErrorCode::Internal,
                    format!("ffmpeg sprite_sheet failed: {e}"),
                )
            })?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            return Err(AppError::new(
                ErrorCode::Internal,
                format!("ffmpeg sprite_sheet failed: {stderr}"),
            ));
        }

        Ok(tmp.into_source())
    }

    async fn scene_detect(&self, source: &FileSource, threshold: f64) -> AppResult<Vec<Timestamp>> {
        let resolved = source.to_local_path().await?;
        let threshold = threshold.clamp(0.0, 1.0);

        let output = tokio::process::Command::new(self.config.ffmpeg_bin())
            .args([
                "-i",
                &resolved.path().to_string_lossy(),
                "-vf",
                &format!("select='gt(scene\\,{threshold})',showinfo"),
                "-f",
                "null",
                "-",
            ])
            .output()
            .await
            .map_err(|e| {
                AppError::new(
                    ErrorCode::Internal,
                    format!("ffmpeg scene_detect failed: {e}"),
                )
            })?;

        // Parse timestamps from showinfo output lines in stderr
        let stderr = String::from_utf8_lossy(&output.stderr);
        let mut timestamps = Vec::new();

        for line in stderr.lines() {
            if let Some(pts_idx) = line.find("pts_time:") {
                let after = &line[pts_idx + 9..];
                let end = after
                    .find(|c: char| !c.is_ascii_digit() && c != '.' && c != '-')
                    .unwrap_or(after.len());
                if let Ok(secs) = after[..end].trim().parse::<f64>() {
                    timestamps.push(Timestamp::from_millis((secs * 1000.0) as u64));
                }
            }
        }

        Ok(timestamps)
    }

    async fn waveform(&self, source: &FileSource, resolution: Resolution) -> AppResult<FileSource> {
        let resolved = source.to_local_path().await?;
        let tmp = rskit_file::TempFile::with_extension("png")?;

        let output = tokio::process::Command::new(self.config.ffmpeg_bin())
            .args([
                "-i",
                &resolved.path().to_string_lossy(),
                "-filter_complex",
                &format!(
                    "showwavespic=s={}x{}:colors=#4080ff",
                    resolution.width, resolution.height,
                ),
                "-frames:v",
                "1",
                "-y",
                &tmp.path().to_string_lossy(),
            ])
            .output()
            .await
            .map_err(|e| {
                AppError::new(ErrorCode::Internal, format!("ffmpeg waveform failed: {e}"))
            })?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            return Err(AppError::new(
                ErrorCode::Internal,
                format!("ffmpeg waveform failed: {stderr}"),
            ));
        }

        Ok(tmp.into_source())
    }
}
