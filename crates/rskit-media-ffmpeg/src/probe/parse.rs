//! FFprobe JSON → [`MediaMetadata`] parsing.
//!
//! Extracts format info, track details (video, audio, subtitle), and metadata
//! tags from ffprobe's JSON output.

use std::collections::HashMap;
use std::time::Duration;

use rskit_errors::AppResult;
use rskit_media::{
    audio::{ChannelLayout, SampleRate},
    codec::Codec,
    color::{ColorRange, ColorSpace, PixelFormat},
    format::Format,
    probe::MediaMetadata,
    spatial::{FrameRate, Resolution},
    track::{AudioTrackInfo, SubtitleTrackInfo, Track, VideoTrackInfo},
    types::{MediaType, TrackKind},
};

/// Parse ffprobe JSON output into a [`MediaMetadata`].
pub(super) fn parse_metadata(json: &serde_json::Value) -> AppResult<MediaMetadata> {
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
        if let Some(track) = parse_stream(i, stream, &mut has_video, &mut has_audio)? {
            tracks.push(track);
        }
    }

    let media_type = if has_video {
        MediaType::Video
    } else if has_audio {
        MediaType::Audio
    } else {
        MediaType::Image
    };

    let format_id = normalize_format_name(format_name);

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

/// Parse a single ffprobe stream entry into a [`Track`].
///
/// Returns `None` for stream types we don't handle (e.g., "data" with no useful info).
fn parse_stream(
    index: usize,
    stream: &serde_json::Value,
    has_video: &mut bool,
    has_audio: &mut bool,
) -> AppResult<Option<Track>> {
    let codec_type = stream
        .get("codec_type")
        .and_then(|v| v.as_str())
        .unwrap_or("");

    let kind = match codec_type {
        "video" => {
            *has_video = true;
            TrackKind::Video
        }
        "audio" => {
            *has_audio = true;
            TrackKind::Audio
        }
        "subtitle" => TrackKind::Subtitle,
        "data" => TrackKind::Data,
        "attachment" => TrackKind::Attachment,
        _ => return Ok(None),
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
        Some(parse_video_info(stream))
    } else {
        None
    };

    let audio = if kind == TrackKind::Audio {
        Some(parse_audio_info(stream))
    } else {
        None
    };

    let subtitle = if kind == TrackKind::Subtitle {
        Some(parse_subtitle_info(stream))
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

    Ok(Some(Track {
        index,
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
    }))
}

fn parse_video_info(stream: &serde_json::Value) -> VideoTrackInfo {
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

    let color_space = stream
        .get("color_space")
        .and_then(|v| v.as_str())
        .map(ColorSpace::from_ffmpeg)
        .filter(|cs| *cs != ColorSpace::Unknown);

    let color_range = stream
        .get("color_range")
        .and_then(|v| v.as_str())
        .map(|r| match r {
            "pc" | "jpeg" | "full" => ColorRange::Full,
            _ => ColorRange::Limited,
        });

    let bit_depth = stream
        .get("bits_per_raw_sample")
        .and_then(|v| v.as_str())
        .and_then(|s| s.parse::<u8>().ok())
        .or_else(|| {
            // Infer from pixel format name
            pix_fmt.as_deref().map(|fmt| {
                if fmt.contains("10le") || fmt.contains("10be") || fmt.ends_with("p10") {
                    10
                } else if fmt.contains("12le") || fmt.contains("12be") || fmt.ends_with("p12") {
                    12
                } else {
                    8
                }
            })
        });

    let profile = stream
        .get("profile")
        .and_then(|v| v.as_str())
        .and_then(rskit_media::CodecProfile::from_ffprobe);

    VideoTrackInfo {
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
    }
}

fn parse_audio_info(stream: &serde_json::Value) -> AudioTrackInfo {
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

    AudioTrackInfo {
        sample_rate: SampleRate::hz(sample_rate),
        channels: layout,
        bit_depth: None,
    }
}

fn parse_subtitle_info(stream: &serde_json::Value) -> SubtitleTrackInfo {
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

    SubtitleTrackInfo {
        format: fmt,
        forced,
    }
}

/// Map common ffprobe format names to our canonical format IDs.
fn normalize_format_name(format_name: &str) -> &str {
    match format_name.split(',').next().unwrap_or("") {
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
    }
}
