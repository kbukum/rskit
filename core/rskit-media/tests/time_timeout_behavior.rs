#![allow(missing_docs)]

use std::time::Duration;

use rskit_media::filter::filters;
use rskit_media::ops::{CropRegion, ResizeMode};
use rskit_media::{
    AudioTrackInfo, ChannelLayout, Codec, FilterTarget, Format, FrameRate, ImageFormat,
    MediaExecutor, MediaMetadata, MediaOp, MediaPipeline, MediaProbe, MediaType, OperationKind,
    OutputConfig, ParamValue, Params, PictureType, Progress, SampleRate, Segment, SilenceInterval,
    SubtitleTrackInfo, ThumbnailConfig, TimeRange, TimeoutCalculator, Timestamp, Track, TrackKind,
    VideoSettings, VideoTrackInfo,
};
use rskit_storage::{FileSink, FileSource};
use tokio_util::sync::CancellationToken;

#[test]
fn timestamps_ranges_and_segments_are_deterministic() {
    let ts = Timestamp::from_hms(1, 2, 3.456);
    assert_eq!(ts.to_ffmpeg_time(), "01:02:03.456");
    assert_eq!(ts.as_millis(), 3_723_456);
    assert_eq!(
        Timestamp::from_micros(ts.as_micros()).as_duration(),
        Duration::from_micros(ts.as_micros())
    );

    let range = TimeRange::from_seconds(1.0, 4.0);
    assert_eq!(range.duration_ms(), 3_000);
    assert!(range.contains(Timestamp::from_seconds(2.0)));
    assert!(range.overlaps(&TimeRange::from_millis(3_000, 5_000)));
    assert_eq!(
        range.merge(&TimeRange::from_seconds(3.0, 5.0)).unwrap().end,
        Timestamp::from_seconds(5.0)
    );
    assert!(range.merge(&TimeRange::from_seconds(5.0, 6.0)).is_none());
    let (left, right) = range.split_at(Timestamp::from_seconds(2.0));
    assert_eq!(left.unwrap().duration_ms(), 1_000);
    assert_eq!(right.unwrap().duration_ms(), 2_000);
    assert_eq!(range.shift(-2_000).start, Timestamp::from_seconds(0.0));

    let segment = Segment::new(range)
        .with_label("intro")
        .with_confidence(2.0)
        .with_meta("scene", 1);
    assert_eq!(segment.label.as_deref(), Some("intro"));
    assert_eq!(segment.confidence, Some(1.0));
    assert_eq!(segment.metadata["scene"], 1);
}

#[test]
fn timeout_calculator_uses_operation_defaults_overrides_and_max_ceiling() {
    assert_eq!(OperationKind::Probe.default_multiplier(), 0.0);
    assert_eq!(
        OperationKind::Probe.default_base_timeout(),
        Duration::from_secs(30)
    );
    assert!(
        OperationKind::MlInference.default_base_timeout()
            > OperationKind::StreamCopy.default_base_timeout()
    );

    let calculator = TimeoutCalculator::default()
        .with_base_timeout(Duration::from_secs(10))
        .with_max_timeout(Duration::from_secs(100))
        .with_multiplier(OperationKind::Transcode, 10.0);
    assert_eq!(
        calculator.calculate(Duration::from_secs(20), OperationKind::Transcode),
        Duration::from_secs(100)
    );
    assert_eq!(
        calculator.calculate(Duration::from_secs(1), OperationKind::Probe),
        Duration::from_secs(30)
    );
}

#[test]
fn filter_params_and_thumbnail_formats_expose_backend_contracts() {
    let params = Params::new()
        .set("strength", 3_i64)
        .set("amount", 1.5_f64)
        .set("mode", "fast")
        .set("enabled", true);
    assert!(matches!(params.get("strength"), Some(ParamValue::Int(3))));
    assert_eq!(params.iter().count(), 4);

    let denoise = filters::denoise(2);
    assert_eq!(denoise.name, "denoise");
    assert_eq!(denoise.target, FilterTarget::Video);
    let sharpen = filters::sharpen(1.25);
    assert_eq!(sharpen.name, "sharpen");
    let custom = filters::custom_video("curves=preset=color_negative");
    assert_eq!(custom.name, "curves=preset=color_negative");

    for (format, codec, extension) in [
        (ImageFormat::Jpeg, "mjpeg", "jpg"),
        (ImageFormat::Png, "png", "png"),
        (ImageFormat::Webp, "libwebp", "webp"),
    ] {
        assert_eq!(format.ffmpeg_codec(), codec);
        assert_eq!(format.extension(), extension);
    }
    let thumbnail = ThumbnailConfig {
        timestamp: 1.5,
        width: Some(320),
        height: None,
        format: ImageFormat::Png,
        quality: None,
    };
    assert_eq!(thumbnail.width, Some(320));
}

#[test]
fn media_operations_classify_track_requirements_and_timeout_kinds() {
    let audio_filter = filters::custom_audio("loudnorm");
    let video_filter = filters::custom_video("scale=1280:720");

    for op in [
        MediaOp::Extract(TimeRange::from_seconds(0.0, 1.0)),
        MediaOp::StripAudio,
        MediaOp::StripVideo,
        MediaOp::SelectTracks(vec![0]),
    ] {
        assert_eq!(op.timeout_kind(), OperationKind::StreamCopy);
    }

    assert!(MediaOp::Filter(audio_filter).requires_audio_track());
    assert!(MediaOp::Filter(video_filter).requires_video_track());
    assert!(MediaOp::Volume(0.5).requires_audio_track());
    assert!(
        MediaOp::Crop(CropRegion::center(
            rskit_media::Resolution::new(1920, 1080),
            1280,
            720
        ))
        .requires_video_track()
    );
    assert!(
        MediaOp::ExtractMany(vec![Segment::new(TimeRange::from_seconds(0.0, 1.0))])
            .needs_stream_hints()
    );
    assert_eq!(
        MediaOp::GenerateThumbnail(ThumbnailConfig {
            timestamp: 0.0,
            width: None,
            height: None,
            format: ImageFormat::Jpeg,
            quality: Some(80),
        })
        .timeout_kind(),
        OperationKind::ThumbnailExtract
    );
}

#[test]
fn output_and_chunk_builders_expose_contract_methods() {
    let video = VideoSettings::new(Codec::new("h264"))
        .with_resolution(rskit_media::Resolution::new(1920, 1080))
        .with_quality(rskit_media::Quality::High)
        .with_bitrate(rskit_media::Bitrate::Variable(2_000_000));
    let output = OutputConfig::new(Format::new(rskit_media::format::MP4))
        .with_video(video)
        .with_strip_metadata()
        .with_param("movflags", "faststart");

    assert!(output.strip_metadata);
    assert_eq!(output.extra["movflags"], "faststart");

    let chunk = rskit_media::ChunkPlan {
        id: rskit_media::ChunkId::from_index(3),
        index: 3,
        range: TimeRange::from_seconds(1.0, 3.5),
        start_is_keyframe: true,
        suggested_timeout: Duration::from_secs(10),
    };
    assert_eq!(chunk.id.to_string(), "chunk-0003");
    assert_eq!(chunk.duration(), Duration::from_millis(2500));
    assert!(rskit_media::ChunkStatus::Completed.is_success());
    assert!(
        !rskit_media::ChunkStatus::Failed {
            message: "failed".to_string(),
            retryable: true,
        }
        .is_success()
    );
    let operation = rskit_media::ChunkedOperation {
        chunks: vec![chunk],
        reassembly: rskit_media::ReassemblyPlan::Concat,
        total_duration: Duration::from_secs(3),
        strategy_name: "fixed".to_string(),
    };
    assert_eq!(operation.chunk_count(), 1);
    assert!(operation.is_single_chunk());
}

struct RecordingExecutor {
    supported: bool,
}

#[async_trait::async_trait]
impl MediaExecutor for RecordingExecutor {
    async fn execute(
        &self,
        source: &FileSource,
        ops: &[MediaOp],
        sink: Option<&FileSink>,
    ) -> rskit_errors::AppResult<FileSource> {
        assert!(!ops.is_empty());
        assert!(sink.is_none());
        Ok(source.clone())
    }

    async fn execute_with_progress(
        &self,
        source: &FileSource,
        ops: &[MediaOp],
        sink: Option<&FileSink>,
        on_progress: Box<dyn Fn(Progress) + Send + Sync>,
    ) -> rskit_errors::AppResult<FileSource> {
        assert!(!ops.is_empty());
        assert!(sink.is_none());
        on_progress(Progress {
            position: Some(Timestamp::from_seconds(1.0)),
            total: Some(Duration::from_secs(2)),
            percent: Some(50.0),
            speed: Some(1.0),
            output_size: Some(42),
            eta: Some(Duration::from_secs(1)),
        });
        Ok(source.clone())
    }

    fn supports(&self, _op: &MediaOp) -> bool {
        self.supported
    }

    fn preview(
        &self,
        _source: &FileSource,
        ops: &[MediaOp],
    ) -> rskit_errors::AppResult<Vec<String>> {
        Ok(vec![format!("{} ops", ops.len())])
    }
}

#[tokio::test]
async fn pipeline_validates_conflicts_estimates_duration_and_executes() {
    let source = FileSource::from_bytes(Vec::from("media"));
    let executor = RecordingExecutor { supported: true };
    let pipeline = MediaPipeline::from(&source)
        .extract(TimeRange::from_seconds(10.0, 20.0))
        .resize(rskit_media::Resolution::new(1280, 720), ResizeMode::Fit)
        .speed(2.0)
        .volume(0.8);

    assert_eq!(pipeline.operations().len(), 4);
    pipeline.validate(&executor).unwrap();
    assert_eq!(
        pipeline.estimated_duration(Duration::from_secs(60)),
        Duration::from_secs(5)
    );

    let returned = pipeline.execute(&executor).await.unwrap();
    assert!(matches!(returned, FileSource::Bytes(_)));

    MediaPipeline::from(&source)
        .extract_many(vec![Segment::new(TimeRange::from_seconds(0.0, 1.0))])
        .execute_with_progress(&executor, |_| {})
        .await
        .unwrap();
    MediaPipeline::from(&source)
        .concat(&source)
        .execute_cancellable(
            &executor,
            Some(Box::new(|progress| {
                assert_eq!(progress.percent, Some(50.0));
            })),
            CancellationToken::new(),
        )
        .await
        .unwrap();
}

#[test]
fn pipeline_validation_rejects_unsupported_and_conflicting_operations() {
    let source = FileSource::from_bytes(Vec::from("media"));
    let unsupported = RecordingExecutor { supported: false };
    assert!(
        MediaPipeline::from(&source)
            .volume(1.0)
            .validate(&unsupported)
            .unwrap_err()
            .message()
            .contains("does not support")
    );

    let supported = RecordingExecutor { supported: true };
    assert!(
        MediaPipeline::from(&source)
            .strip_audio()
            .volume(1.0)
            .validate(&supported)
            .unwrap_err()
            .message()
            .contains("StripAudio")
    );
    assert!(
        MediaPipeline::from(&source)
            .strip_video()
            .resize(rskit_media::Resolution::new(640, 360), ResizeMode::Fit)
            .validate(&supported)
            .unwrap_err()
            .message()
            .contains("StripVideo")
    );
}

struct UnsupportedProbe;

#[async_trait::async_trait]
impl MediaProbe for UnsupportedProbe {
    async fn probe(&self, _source: &FileSource) -> rskit_errors::AppResult<MediaMetadata> {
        Ok(MediaMetadata {
            media_type: MediaType::Video,
            format: Format::new(rskit_media::format::MP4),
            duration: Some(Duration::from_secs(10)),
            size: Some(1024),
            bitrate: Some(8000),
            tracks: vec![
                Track {
                    index: 0,
                    kind: TrackKind::Video,
                    codec: Some(Codec::new("h264")),
                    bitrate: Some(6000),
                    language: None,
                    is_default: true,
                    title: None,
                    duration: Some(Duration::from_secs(10)),
                    video: Some(VideoTrackInfo {
                        resolution: rskit_media::Resolution::new(1920, 1080),
                        frame_rate: Some(FrameRate::fps(30)),
                        pixel_format: None,
                        rotation: None,
                        color_space: None,
                        color_range: None,
                        bit_depth: Some(8),
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
                    codec: Some(Codec::new("aac")),
                    bitrate: Some(2000),
                    language: Some("en".to_string()),
                    is_default: true,
                    title: None,
                    duration: Some(Duration::from_secs(10)),
                    video: None,
                    audio: Some(AudioTrackInfo {
                        sample_rate: SampleRate::dvd(),
                        channels: ChannelLayout::Stereo,
                        bit_depth: Some(16),
                    }),
                    subtitle: None,
                },
                Track {
                    index: 2,
                    kind: TrackKind::Subtitle,
                    codec: None,
                    bitrate: None,
                    language: Some("en".to_string()),
                    is_default: false,
                    title: Some("English".to_string()),
                    duration: None,
                    video: None,
                    audio: None,
                    subtitle: Some(SubtitleTrackInfo {
                        format: "srt".to_string(),
                        forced: false,
                    }),
                },
            ],
            tags: std::collections::HashMap::new(),
            created_at: None,
        })
    }

    async fn thumbnail(
        &self,
        _source: &FileSource,
        _at: Timestamp,
        _resolution: Option<rskit_media::Resolution>,
    ) -> rskit_errors::AppResult<FileSource> {
        Ok(FileSource::from_bytes(Vec::from("thumb")))
    }

    async fn thumbnails(
        &self,
        _source: &FileSource,
        _interval: Duration,
        _resolution: Option<rskit_media::Resolution>,
    ) -> rskit_errors::AppResult<Vec<FileSource>> {
        Ok(vec![FileSource::from_bytes(Vec::from("thumb"))])
    }
}

#[tokio::test]
async fn probe_metadata_accessors_and_default_unsupported_methods_are_typed() {
    let probe = UnsupportedProbe;
    let source = FileSource::from_bytes(Vec::from("media"));
    let metadata = probe.probe(&source).await.unwrap();

    assert!(metadata.has_video());
    assert!(metadata.has_audio());
    assert_eq!(
        metadata.resolution(),
        Some(rskit_media::Resolution::new(1920, 1080))
    );
    assert_eq!(metadata.frame_rate(), Some(FrameRate::fps(30)));
    assert_eq!(metadata.sample_rate(), Some(SampleRate::dvd()));
    assert_eq!(metadata.subtitle_tracks().len(), 1);

    assert!(
        probe
            .thumbnail(&source, Timestamp::from_seconds(1.0), None)
            .await
            .is_ok()
    );
    assert!(
        probe
            .thumbnails(&source, Duration::from_secs(1), None)
            .await
            .is_ok()
    );
    assert!(
        probe
            .sprite_sheet(
                &source,
                Duration::from_secs(1),
                rskit_media::Resolution::new(160, 90),
                4
            )
            .await
            .unwrap_err()
            .message()
            .contains("not supported")
    );
    assert!(probe.scene_detect(&source, 0.4).await.is_err());
    assert!(
        probe
            .waveform(&source, rskit_media::Resolution::new(640, 120))
            .await
            .is_err()
    );
    assert!(probe.keyframes(&source).await.is_err());
    assert!(
        probe
            .silence_detect(&source, Duration::from_millis(500), -40.0)
            .await
            .is_err()
    );
    assert!(probe.chapters(&source).await.is_err());

    let silence = SilenceInterval {
        start: Timestamp::from_seconds(1.0),
        end: Timestamp::from_seconds(3.0),
        duration: Duration::from_secs(2),
    };
    assert_eq!(silence.midpoint(), Timestamp::from_seconds(2.0));
    assert_eq!(silence.as_range().duration(), Duration::from_secs(2));
    assert!(PictureType::from_ffprobe("IDR").is_keyframe());
    assert!(!PictureType::from_ffprobe("B").is_keyframe());
    assert_eq!(PictureType::from_ffprobe("?"), PictureType::Unknown);
    let chapter = rskit_media::Chapter {
        index: 0,
        range: TimeRange::from_seconds(0.0, 5.0),
        title: Some("Intro".to_string()),
    };
    assert_eq!(chapter.duration(), Duration::from_secs(5));
}
