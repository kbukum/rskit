//! Shared helpers for media demo binaries.

use std::fmt::{Debug, Write as _};
use std::path::PathBuf;
use std::time::Duration;

use rskit::media::probe::{MediaMetadata, SilenceInterval};
use rskit::media::{
    Registry, filter::filters, ops::ResizeMode, pipeline::MediaPipeline, presets,
    spatial::Resolution, time::TimeRange, types::TrackKind,
};
use rskit::media_ffmpeg::Config as FfmpegConfig;
use rskit::storage::{FileSink, FileSource};
use rskit::{AppError, AppResult, ErrorCode};

/// Probe demo arguments.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProbeArgs {
    /// Input media path.
    pub path: String,
}

impl ProbeArgs {
    /// Build arguments from CLI values, defaulting to `input.mp4`.
    pub fn parse<I, S>(args: I) -> Self
    where
        I: IntoIterator<Item = S>,
        S: Into<String>,
    {
        let mut args = args.into_iter().map(Into::into);
        Self {
            path: args.next().unwrap_or_else(|| "input.mp4".to_owned()),
        }
    }
}

/// Thumbnail demo arguments.
#[derive(Debug, Clone, PartialEq)]
pub struct ThumbnailArgs {
    /// Input video path.
    pub input: String,
    /// Output image path.
    pub output: String,
    /// Timestamp in seconds.
    pub timestamp_secs: f64,
}

impl ThumbnailArgs {
    /// Build arguments from CLI values, defaulting to `input.mp4`, `thumb.jpg`, and 5 seconds.
    pub fn parse<I, S>(args: I) -> Self
    where
        I: IntoIterator<Item = S>,
        S: Into<String>,
    {
        let mut args = args.into_iter().map(Into::into);
        let input = args.next().unwrap_or_else(|| "input.mp4".to_owned());
        let output = args.next().unwrap_or_else(|| "thumb.jpg".to_owned());
        let timestamp_secs = args
            .next()
            .and_then(|value| value.parse().ok())
            .unwrap_or(5.0);
        Self {
            input,
            output,
            timestamp_secs,
        }
    }
}

/// Transcode demo arguments.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TranscodeArgs {
    /// Input video path.
    pub input: String,
    /// Output video path.
    pub output: String,
}

impl TranscodeArgs {
    /// Build arguments from CLI values, defaulting to `input.mp4` and `output.mp4`.
    pub fn parse<I, S>(args: I) -> Self
    where
        I: IntoIterator<Item = S>,
        S: Into<String>,
    {
        let mut args = args.into_iter().map(Into::into);
        Self {
            input: args.next().unwrap_or_else(|| "input.mp4".to_owned()),
            output: args.next().unwrap_or_else(|| "output.mp4".to_owned()),
        }
    }
}

/// Audio analysis demo arguments.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AudioAnalysisArgs {
    /// Input WAV path.
    pub path: String,
}

impl AudioAnalysisArgs {
    /// Build arguments from CLI values, defaulting to `input.wav`.
    pub fn parse<I, S>(args: I) -> Self
    where
        I: IntoIterator<Item = S>,
        S: Into<String>,
    {
        let mut args = args.into_iter().map(Into::into);
        Self {
            path: args.next().unwrap_or_else(|| "input.wav".to_owned()),
        }
    }
}

/// Build `FFmpeg` demo configuration with local paths confined to the current directory.
///
/// The examples accept `CLI` file paths and pass them to `FFmpeg` subprocesses. Confining
/// those paths to the invocation directory demonstrates the secure-by-default adapter
/// configuration while still keeping the examples easy to run from a media workspace.
pub fn ffmpeg_config() -> AppResult<FfmpegConfig> {
    Ok(FfmpegConfig::default().with_path_root(current_dir()?))
}

/// Probe media metadata and return the formatted demo output.
pub async fn run_probe(args: &ProbeArgs) -> AppResult<String> {
    let source = FileSource::from_path(&args.path);
    let mut registry = Registry::default();
    rskit::media_ffmpeg::register(&mut registry, ffmpeg_config()?)?;
    let probe = registry.probe("ffmpeg")?;
    let info = probe.probe(&source).await?;
    Ok(format_probe_output(&args.path, &info))
}

/// Extract a thumbnail and return the formatted demo output.
pub async fn run_thumbnail(args: &ThumbnailArgs) -> AppResult<String> {
    let source = FileSource::from_path(&args.input);
    let sink = FileSink::Path(args.output.clone().into());
    let mut registry = Registry::default();
    rskit::media_ffmpeg::register(&mut registry, ffmpeg_config()?)?;
    let executor = registry.executor("ffmpeg")?;

    let result = MediaPipeline::from(&source)
        .extract(TimeRange::from_seconds(
            args.timestamp_secs,
            args.timestamp_secs + 0.1,
        ))
        .resize(Resolution::new(640, 360), ResizeMode::Fit)
        .transcode(presets::jpeg())
        .output_to(sink)
        .execute(executor.as_ref())
        .await?;

    Ok(format_thumbnail_output(&args.output, &result))
}

/// Transcode video and return the formatted demo output.
pub async fn run_transcode(args: &TranscodeArgs) -> AppResult<String> {
    let source = FileSource::from_path(&args.input);
    let sink = FileSink::Path(args.output.clone().into());
    let mut registry = Registry::default();
    rskit::media_ffmpeg::register(&mut registry, ffmpeg_config()?)?;
    let executor = registry.executor("ffmpeg")?;

    let result = MediaPipeline::from(&source)
        .extract(TimeRange::from_seconds(0.0, 30.0))
        .resize(Resolution::p720(), ResizeMode::Fit)
        .filter(filters::denoise(3))
        .volume(0.9)
        .transcode(presets::mp4_h264())
        .output_to(sink)
        .execute(executor.as_ref())
        .await?;

    Ok(format_transcode_output(&result))
}

/// Analyze WAV metadata and silence regions, returning formatted demo output.
pub async fn run_audio_analysis(args: &AudioAnalysisArgs) -> AppResult<String> {
    let mut registry = Registry::default();
    rskit::media_audio::register(&mut registry, rskit::media_audio::Config::default())?;
    let probe = registry.probe("audio")?;
    let source = FileSource::from_path(&args.path);
    let metadata = probe.probe(&source).await?;
    let silences = probe
        .silence_detect(&source, Duration::from_millis(500), -40.0)
        .await?;
    Ok(format_audio_analysis_output(
        &args.path, &metadata, &silences,
    ))
}

/// Format media probe metadata for terminal output.
pub fn format_probe_output(path: &str, info: &MediaMetadata) -> String {
    let mut output = String::new();
    let _ = writeln!(output, "=== Media Info: {path} ===");
    let _ = writeln!(output, "Duration : {:?}", info.duration);
    let _ = writeln!(output, "Format   : {:?}", info.format);
    let _ = writeln!(output, "Tracks   : {}", info.tracks.len());

    for track in &info.tracks {
        let _ = writeln!(output, "  [{:?}] codec={:?}", track.kind, track.codec);
        match track.kind {
            TrackKind::Video => {
                if let Some(video) = &track.video {
                    let _ = writeln!(
                        output,
                        "    {}×{} @ {:?} fps, bit_depth={:?}",
                        video.resolution.width,
                        video.resolution.height,
                        video.frame_rate,
                        video.bit_depth,
                    );
                }
            }
            TrackKind::Audio => {
                if let Some(audio) = &track.audio {
                    let _ = writeln!(output, "    {:?}, {:?}", audio.sample_rate, audio.channels);
                }
            }
            TrackKind::Subtitle | TrackKind::Data | TrackKind::Attachment => {}
        }
    }

    output
}

/// Format thumbnail result for terminal output.
pub fn format_thumbnail_output(output: &str, result: &impl Debug) -> String {
    format!("Thumbnail saved to {output}: {result:?}")
}

/// Format transcode result for terminal output.
pub fn format_transcode_output(result: &impl Debug) -> String {
    format!("Transcode complete: {result:?}")
}

/// Format WAV metadata, loudness tags, and silence regions for terminal output.
pub fn format_audio_analysis_output(
    path: &str,
    metadata: &MediaMetadata,
    silences: &[SilenceInterval],
) -> String {
    let mut output = String::new();
    let _ = writeln!(output, "=== WAV Info: {path} ===");
    if let Some(track) = metadata
        .audio_track()
        .and_then(|track| track.audio.as_ref())
    {
        let _ = writeln!(output, "Channels     : {}", track.channels.channel_count());
        let _ = writeln!(output, "Sample rate  : {} Hz", track.sample_rate.0);
        if let Some(bit_depth) = track.bit_depth {
            let _ = writeln!(output, "Bits/sample  : {bit_depth}");
        }
    }
    if let Some(duration) = metadata.duration {
        let _ = writeln!(output, "Duration     : {:.2} s", duration.as_secs_f64());
    }
    if let Some(bitrate) = metadata.bitrate {
        let _ = writeln!(output, "Bitrate      : {bitrate} bps");
    }

    output.push_str("\n=== Loudness ===\n");
    let _ = writeln!(
        output,
        "Peak    : {} dBFS",
        metadata
            .tags
            .get("audio.peak_db")
            .map_or("unknown", String::as_str)
    );
    let _ = writeln!(
        output,
        "RMS     : {} dBFS",
        metadata
            .tags
            .get("audio.rms_db")
            .map_or("unknown", String::as_str)
    );
    let _ = writeln!(
        output,
        "LUFS    : {}",
        metadata
            .tags
            .get("audio.lufs")
            .map_or("unknown", String::as_str)
    );

    let _ = writeln!(output, "\n=== Silence regions ({}) ===", silences.len());
    for (index, silence) in silences.iter().enumerate() {
        let _ = writeln!(
            output,
            "  {index}: {:.2}s - {:.2}s ({:.2}s)",
            silence.start.as_seconds(),
            silence.end.as_seconds(),
            silence.duration.as_secs_f64()
        );
    }

    output
}

fn current_dir() -> AppResult<PathBuf> {
    std::env::current_dir()
        .map_err(|error| AppError::new(ErrorCode::Internal, format!("failed to read cwd: {error}")))
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;
    use std::fs;
    use std::path::PathBuf;
    use std::sync::atomic::{AtomicU64, Ordering};

    use rskit::media::audio::{ChannelLayout, SampleRate};
    use rskit::media::format::{self, Format};
    use rskit::media::probe::SilenceInterval;
    use rskit::media::spatial::{FrameRate, Resolution};
    use rskit::media::time::Timestamp;
    use rskit::media::track::{AudioTrackInfo, Track, VideoTrackInfo};
    use rskit::media::types::{MediaType, TrackKind};

    use super::*;

    #[test]
    fn probe_args_default_and_explicit_path() {
        assert_eq!(ProbeArgs::parse(Vec::<String>::new()).path, "input.mp4");
        assert_eq!(ProbeArgs::parse(["movie.mov"]).path, "movie.mov");
    }

    #[test]
    fn thumbnail_args_parse_defaults_and_explicit_values() {
        assert_eq!(
            ThumbnailArgs::parse(Vec::<String>::new()),
            ThumbnailArgs {
                input: "input.mp4".into(),
                output: "thumb.jpg".into(),
                timestamp_secs: 5.0,
            }
        );
        assert_eq!(
            ThumbnailArgs::parse(["in.mp4", "out.jpg", "12.5"]),
            ThumbnailArgs {
                input: "in.mp4".into(),
                output: "out.jpg".into(),
                timestamp_secs: 12.5,
            }
        );
    }

    #[test]
    fn thumbnail_args_invalid_timestamp_falls_back_to_default() {
        let args = ThumbnailArgs::parse(["in.mp4", "out.jpg", "later"]);
        assert!((args.timestamp_secs - 5.0).abs() < f64::EPSILON);
    }

    #[test]
    fn transcode_and_audio_args_parse_defaults_and_explicit_values() {
        assert_eq!(
            TranscodeArgs::parse(Vec::<String>::new()),
            TranscodeArgs {
                input: "input.mp4".into(),
                output: "output.mp4".into(),
            }
        );
        assert_eq!(
            TranscodeArgs::parse(["raw.mov", "done.mp4"]),
            TranscodeArgs {
                input: "raw.mov".into(),
                output: "done.mp4".into(),
            }
        );
        assert_eq!(
            AudioAnalysisArgs::parse(Vec::<String>::new()).path,
            "input.wav"
        );
        assert_eq!(AudioAnalysisArgs::parse(["voice.wav"]).path, "voice.wav");
    }

    #[test]
    fn probe_output_formats_video_audio_and_subtitle_tracks() {
        let output = format_probe_output("clip.mp4", &sample_metadata(true));

        assert!(output.contains("=== Media Info: clip.mp4 ==="));
        assert!(output.contains("Tracks   : 3"));
        assert!(output.contains("[Video]"));
        assert!(output.contains("1920×1080 @ Some(FrameRate"));
        assert!(output.contains("bit_depth=Some(10)"));
        assert!(output.contains("[Audio]"));
        assert!(output.contains("SampleRate(48000)"));
        assert!(output.contains("Subtitle"));
    }

    #[test]
    fn probe_output_skips_missing_track_details() {
        let mut metadata = sample_metadata(false);
        metadata.tracks.push(Track {
            index: 2,
            kind: TrackKind::Data,
            codec: None,
            bitrate: None,
            language: None,
            is_default: false,
            title: None,
            duration: None,
            video: None,
            audio: None,
            subtitle: None,
        });

        let output = format_probe_output("bare.mkv", &metadata);
        assert!(output.contains("Tracks   : 4"));
        assert!(output.contains("[Data] codec=None"));
    }

    #[test]
    fn thumbnail_and_transcode_outputs_include_debug_results() {
        assert_eq!(
            format_thumbnail_output("thumb.jpg", &vec!["ok"]),
            "Thumbnail saved to thumb.jpg: [\"ok\"]"
        );
        assert_eq!(
            format_transcode_output(&Some("done")),
            "Transcode complete: Some(\"done\")"
        );
    }

    #[test]
    fn audio_analysis_output_formats_metadata_loudness_and_silence() {
        let silence = SilenceInterval {
            start: Timestamp::from_seconds(1.25),
            end: Timestamp::from_seconds(2.5),
            duration: Duration::from_millis(1250),
        };
        let output = format_audio_analysis_output("voice.wav", &sample_metadata(true), &[silence]);

        assert!(output.contains("=== WAV Info: voice.wav ==="));
        assert!(output.contains("Channels     : 2"));
        assert!(output.contains("Sample rate  : 48000 Hz"));
        assert!(output.contains("Bits/sample  : 24"));
        assert!(output.contains("Duration     : 12.35 s"));
        assert!(output.contains("Bitrate      : 320000 bps"));
        assert!(output.contains("Peak    : -1.2 dBFS"));
        assert!(output.contains("RMS     : -18.4 dBFS"));
        assert!(output.contains("LUFS    : -16.0"));
        assert!(output.contains("=== Silence regions (1) ==="));
        assert!(output.contains("0: 1.25s - 2.50s (1.25s)"));
    }

    #[test]
    fn audio_analysis_output_uses_unknown_loudness_defaults() {
        let mut metadata = sample_metadata(false);
        metadata.tags.clear();
        let output = format_audio_analysis_output("empty.wav", &metadata, &[]);

        assert!(output.contains("Peak    : unknown dBFS"));
        assert!(output.contains("RMS     : unknown dBFS"));
        assert!(output.contains("LUFS    : unknown"));
        assert!(output.contains("=== Silence regions (0) ==="));
    }

    #[test]
    fn ffmpeg_config_confines_paths_to_current_directory() {
        let config = ffmpeg_config().unwrap();
        let debug = format!("{config:?}");

        assert!(debug.contains("path_root"));
    }

    #[tokio::test]
    async fn ffmpeg_runners_surface_execution_errors_for_missing_inputs() {
        let probe = run_probe(&ProbeArgs {
            path: "missing-input.mp4".into(),
        })
        .await;
        assert!(probe.is_err());

        let thumbnail = run_thumbnail(&ThumbnailArgs {
            input: "missing-input.mp4".into(),
            output: "missing-thumb.jpg".into(),
            timestamp_secs: 1.0,
        })
        .await;
        assert!(thumbnail.is_err());

        let transcode = run_transcode(&TranscodeArgs {
            input: "missing-input.mp4".into(),
            output: "missing-output.mp4".into(),
        })
        .await;
        assert!(transcode.is_err());
    }

    #[tokio::test]
    async fn audio_analysis_runner_formats_generated_wav() {
        let path = temp_wav_path();
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(&path, silent_wav()).unwrap();

        let output = run_audio_analysis(&AudioAnalysisArgs {
            path: path.to_string_lossy().into_owned(),
        })
        .await
        .unwrap();
        let _ = fs::remove_file(&path);

        assert!(output.contains("=== WAV Info:"));
        assert!(output.contains("Channels     : 1"));
        assert!(output.contains("Sample rate  : 8000 Hz"));
        assert!(output.contains("Bits/sample  : 16"));
        assert!(output.contains("=== Loudness ==="));
    }

    fn sample_metadata(with_details: bool) -> MediaMetadata {
        let mut tags = HashMap::new();
        tags.insert("audio.peak_db".to_owned(), "-1.2".to_owned());
        tags.insert("audio.rms_db".to_owned(), "-18.4".to_owned());
        tags.insert("audio.lufs".to_owned(), "-16.0".to_owned());

        MediaMetadata {
            media_type: MediaType::Video,
            format: Format::new(format::MP4),
            duration: Some(Duration::from_millis(12_345)),
            size: Some(1024),
            bitrate: Some(320_000),
            tracks: vec![
                Track {
                    index: 0,
                    kind: TrackKind::Video,
                    codec: None,
                    bitrate: Some(250_000),
                    language: Some("und".into()),
                    is_default: true,
                    title: None,
                    duration: None,
                    video: with_details.then_some(VideoTrackInfo {
                        resolution: Resolution::new(1920, 1080),
                        frame_rate: Some(FrameRate::new(30000, 1001)),
                        pixel_format: None,
                        rotation: None,
                        color_space: None,
                        color_range: None,
                        bit_depth: Some(10),
                        profile: None,
                        level: None,
                        hdr: None,
                    }),
                    audio: None,
                    subtitle: None,
                },
                Track {
                    index: 1,
                    kind: TrackKind::Audio,
                    codec: None,
                    bitrate: Some(70_000),
                    language: Some("en".into()),
                    is_default: true,
                    title: None,
                    duration: None,
                    video: None,
                    audio: with_details.then_some(AudioTrackInfo {
                        sample_rate: SampleRate::dvd(),
                        channels: ChannelLayout::Stereo,
                        bit_depth: Some(24),
                    }),
                    subtitle: None,
                },
                Track {
                    index: 2,
                    kind: TrackKind::Subtitle,
                    codec: None,
                    bitrate: None,
                    language: Some("en".into()),
                    is_default: false,
                    title: Some("English".into()),
                    duration: None,
                    video: None,
                    audio: None,
                    subtitle: None,
                },
            ],
            tags,
            created_at: None,
        }
    }

    fn temp_wav_path() -> PathBuf {
        static NEXT_ID: AtomicU64 = AtomicU64::new(0);
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join(format!(
            "target/rskit-media-demo-{}-{}.wav",
            std::process::id(),
            NEXT_ID.fetch_add(1, Ordering::Relaxed)
        ))
    }

    fn silent_wav() -> Vec<u8> {
        let sample_count = 800_u32;
        let data_len = sample_count * 2;
        let mut bytes = Vec::new();
        bytes.extend_from_slice(b"RIFF");
        bytes.extend_from_slice(&(36 + data_len).to_le_bytes());
        bytes.extend_from_slice(b"WAVEfmt ");
        bytes.extend_from_slice(&16_u32.to_le_bytes());
        bytes.extend_from_slice(&1_u16.to_le_bytes());
        bytes.extend_from_slice(&1_u16.to_le_bytes());
        bytes.extend_from_slice(&8000_u32.to_le_bytes());
        bytes.extend_from_slice(&16000_u32.to_le_bytes());
        bytes.extend_from_slice(&2_u16.to_le_bytes());
        bytes.extend_from_slice(&16_u16.to_le_bytes());
        bytes.extend_from_slice(b"data");
        bytes.extend_from_slice(&data_len.to_le_bytes());
        bytes.resize(bytes.len() + usize::try_from(data_len).unwrap(), 0);
        bytes
    }
}
