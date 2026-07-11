//! FFmpeg command builder — compiles MediaOp list into FFmpeg CLI arguments.

mod optimize;
mod runner;

use std::ffi::OsString;
use std::time::Duration;

use rskit_errors::AppResult;
use rskit_media::{ops::*, registry::Registry, time::Timestamp};
use rskit_storage::{FileSink, FileSource};

use crate::{compilers::CompileContext, config::FfmpegConfig};

/// FFmpeg input specification (source file, optional seek/duration).
pub(crate) struct FfmpegInput {
    pub source: FileSource,
    pub seek_to: Option<Timestamp>,
    pub duration: Option<Duration>,
}

/// Optional hints about the source media, used to make smarter compilation decisions.
///
/// When available (e.g., from a prior probe), these hints let the command builder
/// generate more accurate filter graphs. Without hints, the builder uses conservative
/// defaults.
#[derive(Debug, Clone, Default)]
pub(crate) struct SourceHints {
    /// Whether the primary source has at least one audio stream.
    /// `None` means unknown — the builder will assume audio exists (common case).
    pub has_audio: Option<bool>,
}

/// Compiled FFmpeg command ready for execution.
///
/// Holds all inputs, filters, output options, and global flags. Use
/// [`compile`](FfmpegCommand::compile) to construct from media operations.
pub(crate) struct FfmpegCommand {
    /// Input file specifications.
    pub(crate) inputs: Vec<FfmpegInput>,
    /// Video filter expressions (joined with `,` into a `-vf` chain).
    pub(crate) video_filters: Vec<String>,
    /// Audio filter expressions (joined with `,` into an `-af` chain).
    pub(crate) audio_filters: Vec<String>,
    /// Additional output options (codec flags, maps, etc.).
    pub(crate) output_opts: Vec<String>,
    /// Complex filter graph (used for concat, overlay, etc.).
    pub(crate) complex_filter: Option<String>,
    /// Global options applied before inputs (`-y`, `-loglevel`, etc.).
    pub(crate) global_opts: Vec<String>,
    /// Temp files kept alive for the duration of the command (e.g., subtitle files).
    #[allow(dead_code)]
    pub(crate) temp_files: Vec<rskit_storage::TempFile>,
}

impl FfmpegCommand {
    /// Compile a list of media operations into an FFmpeg command.
    ///
    /// Uses default [`SourceHints`] (assumes audio present). For more accurate
    /// compilation when stream info is known, use [`compile_with_hints`](Self::compile_with_hints).
    pub(crate) fn compile(
        source: &FileSource,
        ops: &[MediaOp],
        sink: Option<&FileSink>,
        config: &FfmpegConfig,
        registry: &Registry,
    ) -> AppResult<Self> {
        Self::compile_with_hints(source, ops, sink, config, registry, &SourceHints::default())
    }

    /// Compile operations into an FFmpeg command, using source hints for smarter output.
    pub(crate) fn compile_with_hints(
        source: &FileSource,
        ops: &[MediaOp],
        sink: Option<&FileSink>,
        config: &FfmpegConfig,
        registry: &Registry,
        hints: &SourceHints,
    ) -> AppResult<Self> {
        // Optimize and validate
        let ops = Self::optimize_ops(ops);
        Self::validate_ops(&ops)?;

        let _ = sink; // used for output path in future
        let mut cmd = Self {
            inputs: vec![FfmpegInput {
                source: source.clone(),
                seek_to: None,
                duration: None,
            }],
            video_filters: Vec::new(),
            audio_filters: Vec::new(),
            output_opts: Vec::new(),
            complex_filter: None,
            global_opts: Vec::new(),
            temp_files: Vec::new(),
        };

        if config.overwrite {
            cmd.global_opts.push("-y".into());
        }
        cmd.global_opts
            .extend(["-loglevel".into(), config.log_level.as_ffmpeg_arg().into()]);
        cmd.global_opts
            .extend(["-progress".into(), "pipe:2".into()]);

        if let Some(threads) = config.threads {
            cmd.global_opts
                .extend(["-threads".into(), threads.to_string()]);
        }

        if let Some(hw) = &config.hw_accel
            && let Some(arg) = hw.ffmpeg_arg()
        {
            cmd.global_opts.extend(["-hwaccel".into(), arg.into()]);
        }

        // Force a specific input video decoder (e.g., libdav1d for software AV1 decode)
        if let Some(decoder) = &config.input_video_decoder {
            cmd.global_opts.extend(["-c:v".into(), decoder.clone()]);
        }

        let mut ctx = CompileContext {
            cmd: &mut cmd,
            config,
            hints,
            registry,
        };

        use crate::compilers;
        for op in &ops {
            match op {
                MediaOp::Extract(range) => compilers::extract::compile_extract(&mut ctx, range)?,
                MediaOp::ExtractMany(segs) => {
                    compilers::extract::compile_extract_many(&mut ctx, segs)?;
                }
                MediaOp::Resize(r) => compilers::spatial::compile_resize(&mut ctx, r)?,
                MediaOp::Crop(c) => compilers::spatial::compile_crop(&mut ctx, c)?,
                MediaOp::Rotate(r) => compilers::spatial::compile_rotate(&mut ctx, r)?,
                MediaOp::Flip(d) => compilers::spatial::compile_flip(&mut ctx, d)?,
                MediaOp::Pad(p) => compilers::spatial::compile_pad(&mut ctx, p)?,
                MediaOp::Speed(f) => compilers::temporal::compile_speed(&mut ctx, *f)?,
                MediaOp::Reverse => compilers::temporal::compile_reverse(&mut ctx)?,
                MediaOp::Volume(f) => compilers::audio::compile_volume(&mut ctx, *f)?,
                MediaOp::NormalizeAudio => compilers::audio::compile_normalize_audio(&mut ctx)?,
                MediaOp::FadeIn(d) => compilers::audio::compile_fade_in(&mut ctx, d)?,
                MediaOp::FadeOut(d) => compilers::audio::compile_fade_out(&mut ctx, d)?,
                MediaOp::StripAudio => compilers::audio::compile_strip_audio(&mut ctx)?,
                MediaOp::StripVideo => compilers::audio::compile_strip_video(&mut ctx)?,
                MediaOp::Filter(f) => compilers::filter::compile_filter(&mut ctx, f)?,
                MediaOp::Overlay(o) => compilers::compose::compile_overlay(&mut ctx, o)?,
                MediaOp::Concat(c) => compilers::compose::compile_concat(&mut ctx, c)?,
                MediaOp::ReplaceAudio(r) => {
                    compilers::compose::compile_replace_audio(&mut ctx, r)?;
                }
                MediaOp::MixAudio(m) => compilers::compose::compile_mix_audio(&mut ctx, m)?,
                MediaOp::Transcode(c) => compilers::transcode::compile_transcode(&mut ctx, c)?,
                MediaOp::SelectTracks(i) => {
                    compilers::tracks::compile_select_tracks(&mut ctx, i)?;
                }
                MediaOp::SelectTracksByKind(k) => {
                    compilers::tracks::compile_select_tracks_by_kind(&mut ctx, k)?;
                }
                MediaOp::BurnSubtitles(s) => {
                    compilers::subtitle::compile_burn_subtitles(&mut ctx, s)?;
                }
                MediaOp::ApplyFilter(c) => compilers::visual::compile_apply_filter(&mut ctx, c)?,
                MediaOp::AddOverlay(c) => {
                    compilers::overlay::compile_add_overlay(&mut ctx, c)?;
                }
                MediaOp::GenerateThumbnail(c) => {
                    compilers::thumbnail::compile_generate_thumbnail(&mut ctx, c)?;
                }
                MediaOp::DetectScenes(c) => {
                    compilers::scene_detect::compile_detect_scenes(&mut ctx, c)?;
                }
                MediaOp::AddSubtitles(c) => {
                    compilers::subtitle::compile_add_subtitles(&mut ctx, c)?;
                }
                MediaOp::Upscale(_) => compilers::ai::compile_upscale()?,
                MediaOp::Interpolate(_) => compilers::ai::compile_interpolate()?,
                _ => {
                    return Err(rskit_errors::AppError::new(
                        rskit_errors::ErrorCode::InvalidInput,
                        format!("unsupported operation: {op:?}"),
                    ));
                }
            }
        }

        for input in &mut cmd.inputs {
            if let FileSource::Path(path) = &input.source {
                input.source = FileSource::Path(crate::paths::confine_source_path(config, path)?);
            }
        }

        Ok(cmd)
    }

    /// Build the final FFmpeg CLI argument list (excluding the output path).
    pub(crate) fn to_args(&self) -> Vec<String> {
        self.to_os_args()
            .into_iter()
            .map(|arg| arg.to_string_lossy().into_owned())
            .collect()
    }

    /// Build the final FFmpeg CLI argument list preserving OS-native path arguments.
    pub(crate) fn to_os_args(&self) -> Vec<OsString> {
        let mut args = Vec::new();

        args.extend(self.global_opts.iter().map(OsString::from));

        for input in &self.inputs {
            if let Some(seek) = &input.seek_to {
                args.push(OsString::from("-ss"));
                args.push(OsString::from(seek.to_ffmpeg_time()));
            }
            if let Some(dur) = &input.duration {
                args.push(OsString::from("-t"));
                args.push(OsString::from(format!("{:.3}", dur.as_secs_f64())));
            }
            args.push(OsString::from("-i"));
            match &input.source {
                FileSource::Path(path) => args.push(path.as_os_str().to_os_string()),
                FileSource::Temp(temp) => args.push(temp.path().as_os_str().to_os_string()),
                _ => args.push(OsString::from("pipe:0")),
            }
        }

        if let Some(complex) = &self.complex_filter {
            args.push(OsString::from("-filter_complex"));
            args.push(OsString::from(complex));
        } else {
            if !self.video_filters.is_empty() {
                args.push(OsString::from("-vf"));
                args.push(OsString::from(self.video_filters.join(",")));
            }
            if !self.audio_filters.is_empty() {
                args.push(OsString::from("-af"));
                args.push(OsString::from(self.audio_filters.join(",")));
            }
        }

        args.extend(self.output_opts.iter().map(OsString::from));
        args
    }
}

// ── Tests: golden arg verification for each operation ───────────────

#[cfg(test)]
mod tests {
    use super::*;
    use rskit_media::{
        TrackKind,
        audio::{ChannelLayout, SampleRate},
        codec::{Codec, CodecLevel, CodecProfile, audio as audio_codec, video as video_codec},
        format::Format,
        ops::{
            ColorAdjustments, ConcatOp, CropRegion, FilterConfig, FilterPreset, FlipDirection,
            ImageFormat, InterpolateConfig, InterpolateModel, MixAudioOp, OverlayOp,
            OverlayPosition, ResizeMode, ResizeOp, Rotation, SceneDetectConfig, SubtitleFormat,
            SubtitleSource, ThumbnailConfig, UpscaleConfig, UpscaleModel,
        },
        output::{
            AudioSettings, Bitrate, DashConfig, EncodingSpeed, OutputConfig, Quality, RtmpConfig,
            StreamingConfig, VideoSettings,
        },
        spatial::{FrameRate, Resolution},
        time::{Segment, TimeRange, Timestamp},
    };

    fn default_config() -> FfmpegConfig {
        FfmpegConfig {
            overwrite: true,
            ..FfmpegConfig::default()
        }
    }

    #[test]
    fn compile_rejects_input_outside_configured_path_root() {
        let root = rskit_storage::TempDir::new().unwrap();
        let outside = rskit_storage::TempDir::new().unwrap();
        let outside_path = outside.path().join("input.mp4");
        std::fs::write(&outside_path, b"not real media").unwrap();

        let config = default_config().with_path_root(root.path());
        let result = FfmpegCommand::compile(
            &FileSource::from_path(&outside_path),
            &[],
            None,
            &config,
            &Registry::default(),
        );
        let error = match result {
            Ok(_) => panic!("outside path should be rejected"),
            Err(error) => error,
        };

        assert_eq!(error.code(), rskit_errors::ErrorCode::InvalidInput);
    }

    #[test]
    fn compile_canonicalizes_input_inside_configured_path_root() {
        let root = rskit_storage::TempDir::new().unwrap();
        let input = root.path().join("input.mp4");
        std::fs::write(&input, b"not real media").unwrap();

        let config = default_config().with_path_root(root.path());
        let command = FfmpegCommand::compile(
            &FileSource::from_path("input.mp4"),
            &[],
            None,
            &config,
            &Registry::default(),
        )
        .unwrap();

        match &command.inputs[0].source {
            FileSource::Path(path) => assert_eq!(path, &std::fs::canonicalize(input).unwrap()),
            other => panic!("expected path source, got {other:?}"),
        }
    }

    fn default_registry() -> Registry {
        Registry::default()
    }

    fn compile_args(ops: &[MediaOp]) -> Vec<String> {
        let source = FileSource::from_path("/tmp/input.mp4");
        let cmd =
            FfmpegCommand::compile(&source, ops, None, &default_config(), &default_registry())
                .expect("compile");
        cmd.to_args()
    }

    fn compile_args_with_config(ops: &[MediaOp], config: &FfmpegConfig) -> Vec<String> {
        let source = FileSource::from_path("/tmp/input.mp4");
        FfmpegCommand::compile(&source, ops, None, config, &default_registry())
            .expect("compile")
            .to_args()
    }

    // ── Golden tests: verify exact CLI args for each operation ────────

    #[test]
    fn golden_resize_exact() {
        let ops = vec![MediaOp::Resize(ResizeOp {
            resolution: Resolution::new(1280, 720),
            mode: ResizeMode::Exact,
        })];
        let args = compile_args(&ops);
        assert!(args.contains(&"-vf".to_string()), "args: {args:?}");
        let vf_idx = args.iter().position(|a| a == "-vf").unwrap();
        assert_eq!(args[vf_idx + 1], "scale=1280:720");
    }

    #[test]
    fn golden_resize_fit() {
        let ops = vec![MediaOp::Resize(ResizeOp {
            resolution: Resolution::new(640, 480),
            mode: ResizeMode::Fit,
        })];
        let args = compile_args(&ops);
        let vf_idx = args.iter().position(|a| a == "-vf").unwrap();
        let filter = &args[vf_idx + 1];
        assert!(
            filter.contains("force_original_aspect_ratio=decrease"),
            "got: {filter}"
        );
        assert!(filter.contains("pad=640:480"), "got: {filter}");
    }

    #[test]
    fn golden_resize_fill() {
        let ops = vec![MediaOp::Resize(ResizeOp {
            resolution: Resolution::new(640, 480),
            mode: ResizeMode::Fill,
        })];
        let args = compile_args(&ops);
        let vf_idx = args.iter().position(|a| a == "-vf").unwrap();
        let filter = &args[vf_idx + 1];
        assert!(
            filter.contains("force_original_aspect_ratio=increase"),
            "got: {filter}"
        );
        assert!(filter.contains("crop=640:480"), "got: {filter}");
    }

    #[test]
    fn golden_crop() {
        let ops = vec![MediaOp::Crop(CropRegion::new(10, 20, 640, 480))];
        let args = compile_args(&ops);
        let vf_idx = args.iter().position(|a| a == "-vf").unwrap();
        assert_eq!(args[vf_idx + 1], "crop=640:480:10:20");
    }

    #[test]
    fn golden_rotate_90() {
        let ops = vec![MediaOp::Rotate(Rotation::Degrees90)];
        let args = compile_args(&ops);
        let vf_idx = args.iter().position(|a| a == "-vf").unwrap();
        assert_eq!(args[vf_idx + 1], "transpose=1");
    }

    #[test]
    fn golden_rotate_180() {
        let ops = vec![MediaOp::Rotate(Rotation::Degrees180)];
        let args = compile_args(&ops);
        let vf_idx = args.iter().position(|a| a == "-vf").unwrap();
        assert_eq!(args[vf_idx + 1], "hflip,vflip");
    }

    #[test]
    fn golden_flip_horizontal() {
        let ops = vec![MediaOp::Flip(FlipDirection::Horizontal)];
        let args = compile_args(&ops);
        let vf_idx = args.iter().position(|a| a == "-vf").unwrap();
        assert_eq!(args[vf_idx + 1], "hflip");
    }

    #[test]
    fn golden_flip_both() {
        let ops = vec![MediaOp::Flip(FlipDirection::Both)];
        let args = compile_args(&ops);
        let vf_idx = args.iter().position(|a| a == "-vf").unwrap();
        // Both hflip and vflip joined
        assert_eq!(args[vf_idx + 1], "hflip,vflip");
    }

    #[test]
    fn golden_extract_time_range() {
        let ops = vec![MediaOp::Extract(TimeRange::from_seconds(10.0, 30.0))];
        let args = compile_args(&ops);
        // Should have -ss and -t
        assert!(
            args.contains(&"-ss".to_string()),
            "missing -ss in: {args:?}"
        );
        assert!(args.contains(&"-t".to_string()), "missing -t in: {args:?}");
    }

    #[test]
    fn golden_speed() {
        let ops = vec![MediaOp::Speed(2.0)];
        let args = compile_args(&ops);
        let vf_idx = args.iter().position(|a| a == "-vf").unwrap();
        assert_eq!(args[vf_idx + 1], "setpts=PTS/2");
        let af_idx = args.iter().position(|a| a == "-af").unwrap();
        assert!(
            args[af_idx + 1].contains("atempo"),
            "got: {}",
            args[af_idx + 1]
        );
    }

    #[test]
    fn golden_volume() {
        let ops = vec![MediaOp::Volume(0.5)];
        let args = compile_args(&ops);
        let af_idx = args.iter().position(|a| a == "-af").unwrap();
        assert_eq!(args[af_idx + 1], "volume=0.5");
    }

    #[test]
    fn golden_strip_audio() {
        let ops = vec![MediaOp::StripAudio];
        let args = compile_args(&ops);
        assert!(args.contains(&"-an".to_string()));
    }

    #[test]
    fn golden_strip_video() {
        let ops = vec![MediaOp::StripVideo];
        let args = compile_args(&ops);
        assert!(args.contains(&"-vn".to_string()));
    }

    #[test]
    fn golden_reverse() {
        let ops = vec![MediaOp::Reverse];
        let args = compile_args(&ops);
        let vf_idx = args.iter().position(|a| a == "-vf").unwrap();
        assert_eq!(args[vf_idx + 1], "reverse");
        let af_idx = args.iter().position(|a| a == "-af").unwrap();
        assert_eq!(args[af_idx + 1], "areverse");
    }

    #[test]
    fn golden_normalize_audio() {
        let ops = vec![MediaOp::NormalizeAudio];
        let args = compile_args(&ops);
        let af_idx = args.iter().position(|a| a == "-af").unwrap();
        assert_eq!(args[af_idx + 1], "loudnorm");
    }

    #[test]
    fn speed_compiler_chains_atempo_filters_for_extreme_factors() {
        let fast = compile_args(&[MediaOp::Speed(8.0)]);
        let af_idx = fast.iter().position(|a| a == "-af").unwrap();
        assert_eq!(fast[af_idx + 1], "atempo=2.0,atempo=2.0,atempo=2");

        let slow = compile_args(&[MediaOp::Speed(0.125)]);
        let af_idx = slow.iter().position(|a| a == "-af").unwrap();
        assert_eq!(slow[af_idx + 1], "atempo=0.5,atempo=0.5,atempo=0.5");
    }

    #[test]
    fn fade_operations_add_matching_audio_and_video_filters() {
        let args = compile_args(&[
            MediaOp::FadeIn(Duration::from_millis(1500)),
            MediaOp::FadeOut(Duration::from_secs(2)),
        ]);

        let vf_idx = args.iter().position(|a| a == "-vf").unwrap();
        assert_eq!(args[vf_idx + 1], "fade=t=in:d=1.5,fade=t=out:d=2");
        let af_idx = args.iter().position(|a| a == "-af").unwrap();
        assert_eq!(args[af_idx + 1], "afade=t=in:d=1.5,afade=t=out:d=2");
    }

    #[test]
    fn spatial_variants_compile_to_expected_filters() {
        let args = compile_args(&[
            MediaOp::Resize(ResizeOp {
                resolution: Resolution::new(640, 360),
                mode: ResizeMode::FitWidth,
            }),
            MediaOp::Resize(ResizeOp {
                resolution: Resolution::new(640, 360),
                mode: ResizeMode::FitHeight,
            }),
            MediaOp::Rotate(Rotation::Degrees270),
            MediaOp::Rotate(Rotation::Arbitrary(45.0)),
            MediaOp::Flip(FlipDirection::Vertical),
            MediaOp::Pad(PadOp {
                width: 800,
                height: 600,
                color: "black".into(),
            }),
        ]);

        let vf_idx = args.iter().position(|a| a == "-vf").unwrap();
        let vf = &args[vf_idx + 1];
        for expected in [
            "scale=-2:360",
            "transpose=2",
            "rotate=45*PI/180",
            "vflip",
            "pad=800:600:(ow-iw)/2:(oh-ih)/2:black",
        ] {
            assert!(vf.contains(expected), "missing {expected} in {vf}");
        }
    }

    #[test]
    fn extract_many_compiles_single_and_multi_segment_modes() {
        let one = compile_args(&[MediaOp::ExtractMany(vec![
            Segment::new(TimeRange::from_seconds(1.0, 2.0)).with_label("intro"),
        ])]);
        assert!(one.contains(&"-ss".to_string()), "missing seek in {one:?}");
        assert!(
            one.contains(&"-t".to_string()),
            "missing duration in {one:?}"
        );

        let command = FfmpegCommand::compile_with_hints(
            &FileSource::from_path("/tmp/input.mp4"),
            &[MediaOp::ExtractMany(vec![
                Segment::new(TimeRange::from_seconds(0.0, 1.0)),
                Segment::new(TimeRange::from_seconds(3.0, 4.5)),
            ])],
            None,
            &default_config(),
            &default_registry(),
            &SourceHints {
                has_audio: Some(false),
            },
        )
        .unwrap();
        assert_eq!(command.inputs.len(), 2);
        assert_eq!(
            command.complex_filter.as_deref(),
            Some("[0:v][1:v]concat=n=2:v=1:a=0[outv]")
        );
        assert_eq!(command.output_opts, vec!["-map", "[outv]"]);

        let empty = FfmpegCommand::compile(
            &FileSource::from_path("/tmp/input.mp4"),
            &[MediaOp::ExtractMany(Vec::new())],
            None,
            &default_config(),
            &default_registry(),
        );
        let error = match empty {
            Ok(_) => panic!("empty extract-many should be rejected"),
            Err(error) => error,
        };
        assert_eq!(error.code(), rskit_errors::ErrorCode::InvalidInput);
    }

    #[test]
    fn visual_filter_presets_and_custom_adjustments_chain() {
        let args = compile_args(&[
            MediaOp::ApplyFilter(FilterConfig {
                preset: FilterPreset::BW,
                intensity: 1.0,
                custom_params: None,
            }),
            MediaOp::ApplyFilter(FilterConfig {
                preset: FilterPreset::Cinematic,
                intensity: 0.5,
                custom_params: None,
            }),
            MediaOp::ApplyFilter(FilterConfig {
                preset: FilterPreset::Custom,
                intensity: 0.5,
                custom_params: Some(ColorAdjustments {
                    brightness: Some(0.2),
                    contrast: Some(0.4),
                    saturation: Some(0.6),
                    temperature: None,
                    gamma: Some(1.1),
                }),
            }),
        ]);

        let vf_idx = args.iter().position(|a| a == "-vf").unwrap();
        let vf = &args[vf_idx + 1];
        assert!(vf.contains("hue=s=0"));
        assert!(vf.contains("eq=brightness=-0.025:contrast=1.05:saturation=0.95"));
        assert!(vf.contains("eq=brightness=0.1:contrast=1.2:saturation=1.3:gamma=1.1"));
    }

    #[test]
    fn thumbnail_compiler_supports_width_only_and_height_only_scaling() {
        let width_only = compile_args(&[MediaOp::GenerateThumbnail(ThumbnailConfig {
            timestamp: 1.25,
            width: Some(320),
            height: None,
            quality: Some(3),
            format: ImageFormat::Jpeg,
        })]);
        assert!(width_only.contains(&"-ss".to_string()));
        assert!(width_only.windows(2).any(|w| w == ["-vframes", "1"]));
        assert!(width_only.windows(2).any(|w| w == ["-q:v", "3"]));
        let vf_idx = width_only.iter().position(|a| a == "-vf").unwrap();
        assert_eq!(width_only[vf_idx + 1], "scale=320:-2");

        let height_only = compile_args(&[MediaOp::GenerateThumbnail(ThumbnailConfig {
            timestamp: 2.0,
            width: None,
            height: Some(180),
            quality: None,
            format: ImageFormat::Png,
        })]);
        let vf_idx = height_only.iter().position(|a| a == "-vf").unwrap();
        assert_eq!(height_only[vf_idx + 1], "scale=-2:180");
        assert!(height_only.windows(2).any(|w| w == ["-f", "image2"]));
    }

    #[test]
    fn golden_multiple_video_filters_chained() {
        let ops = vec![
            MediaOp::Resize(ResizeOp {
                resolution: Resolution::p720(),
                mode: ResizeMode::Exact,
            }),
            MediaOp::Crop(CropRegion::new(0, 0, 640, 360)),
            MediaOp::Flip(FlipDirection::Horizontal),
        ];
        let args = compile_args(&ops);
        let vf_idx = args.iter().position(|a| a == "-vf").unwrap();
        let vf = &args[vf_idx + 1];
        // All filters joined with commas
        assert!(vf.contains("scale=1280:720"), "got: {vf}");
        assert!(vf.contains("crop=640:360:0:0"), "got: {vf}");
        assert!(vf.contains("hflip"), "got: {vf}");
        assert_eq!(vf.matches(',').count(), 2, "expected 2 commas in: {vf}");
    }

    #[test]
    fn golden_global_opts() {
        let args = compile_args(&[]);
        assert!(
            args.contains(&"-y".to_string()),
            "should have -y (overwrite)"
        );
        assert!(
            args.contains(&"-loglevel".to_string()),
            "should have -loglevel"
        );
        assert!(
            args.contains(&"-progress".to_string()),
            "should have -progress"
        );
    }

    #[test]
    fn golden_input_path() {
        let args = compile_args(&[]);
        let i_idx = args.iter().position(|a| a == "-i").unwrap();
        assert_eq!(args[i_idx + 1], "/tmp/input.mp4");
    }

    #[test]
    fn global_options_follow_configured_execution_knobs() {
        let config = default_config()
            .with_overwrite(false)
            .with_debug_log_level()
            .with_threads(4)
            .with_software_decode()
            .with_input_video_decoder("libdav1d");

        let args = compile_args_with_config(&[], &config);

        assert!(!args.contains(&"-y".to_string()));
        assert!(args.windows(2).any(|w| w == ["-loglevel", "debug"]));
        assert!(args.windows(2).any(|w| w == ["-threads", "4"]));
        assert!(args.windows(2).any(|w| w == ["-hwaccel", "none"]));
        assert!(args.windows(2).any(|w| w == ["-c:v", "libdav1d"]));
    }

    #[test]
    fn non_path_sources_are_read_from_stdin_pipe() {
        let source = FileSource::from_bytes(bytes::Bytes::from_static(b"media"));
        let cmd =
            FfmpegCommand::compile(&source, &[], None, &default_config(), &default_registry())
                .unwrap();
        let args = cmd.to_args();

        let i_idx = args.iter().position(|a| a == "-i").unwrap();
        assert_eq!(args[i_idx + 1], "pipe:0");
    }

    #[test]
    fn temp_sources_use_managed_temp_file_path() {
        let temp = rskit_storage::TempFile::with_extension("mp4").unwrap();
        std::fs::write(temp.path(), b"media").unwrap();
        let source = FileSource::Temp(temp);

        let cmd =
            FfmpegCommand::compile(&source, &[], None, &default_config(), &default_registry())
                .unwrap();
        let args = cmd.to_args();

        let i_idx = args.iter().position(|a| a == "-i").unwrap();
        assert_ne!(args[i_idx + 1], "pipe:0");
        assert!(std::path::Path::new(&args[i_idx + 1]).exists());
    }

    #[test]
    fn concat_respects_audio_presence_hints() {
        let source = FileSource::from_path("/tmp/input.mp4");
        let op = MediaOp::Concat(ConcatOp {
            source: FileSource::from_path("/tmp/second.mp4"),
            transition: None,
        });
        let command = FfmpegCommand::compile_with_hints(
            &source,
            &[op],
            None,
            &default_config(),
            &default_registry(),
            &SourceHints {
                has_audio: Some(false),
            },
        )
        .unwrap();

        assert_eq!(
            command.complex_filter.as_deref(),
            Some("[0:v][1:v]concat=n=2:v=1:a=0")
        );
    }

    #[test]
    fn composition_operations_add_inputs_and_filter_graphs() {
        let overlay = compile_args(&[MediaOp::Overlay(OverlayOp {
            source: FileSource::from_path("/tmp/logo.png"),
            position: OverlayPosition::BottomRight(10, 20),
            opacity: 1.0,
            time_range: None,
            scale: None,
        })]);
        assert!(
            overlay
                .windows(2)
                .any(|w| w == ["-filter_complex", "[0][1]overlay=W-w-10:H-h-20"])
        );

        let replace = compile_args(&[MediaOp::ReplaceAudio(rskit_media::ops::ReplaceAudioOp {
            audio_source: FileSource::from_path("/tmp/audio.wav"),
            offset: Some(Timestamp::from_seconds(1.0)),
        })]);
        assert!(replace.windows(2).any(|w| w == ["-map", "0:v"]));
        assert!(replace.windows(2).any(|w| w == ["-map", "1:a"]));

        let mixed = compile_args(&[MediaOp::MixAudio(MixAudioOp {
            audio_source: FileSource::from_path("/tmp/music.wav"),
            volume: 0.5,
            offset: None,
        })]);
        assert!(mixed.windows(2).any(|w| w
            == [
                "-filter_complex",
                "[0:a][1:a]amix=inputs=2:duration=first:dropout_transition=3"
            ]));
    }

    #[test]
    fn overlay_positions_compile_all_coordinate_variants() {
        for (position, expected) in [
            (OverlayPosition::TopRight(10, 20), "W-w-10:20"),
            (OverlayPosition::BottomLeft(10, 20), "10:H-h-20"),
            (OverlayPosition::BottomRight(10, 20), "W-w-10:H-h-20"),
            (
                OverlayPosition::Custom {
                    x: "main_w-overlay_w-5".into(),
                    y: "main_h-overlay_h-5".into(),
                },
                "main_w-overlay_w-5:main_h-overlay_h-5",
            ),
        ] {
            let args = compile_args(&[MediaOp::Overlay(OverlayOp {
                source: FileSource::from_path("overlay.png"),
                position,
                opacity: 1.0,
                time_range: None,
                scale: None,
            })]);
            assert!(
                args.iter().any(|arg| arg.contains(expected)),
                "args: {args:?}"
            );
        }
    }

    #[test]
    fn audio_filter_targets_audio_filter_chain() {
        let args = compile_args(&[MediaOp::Filter(rskit_media::filter::filters::high_pass(
            120,
        ))]);

        assert!(args.windows(2).any(|w| w == ["-af", "highpass=f=120"]));
    }

    #[test]
    fn track_selection_maps_indices_and_supported_track_kinds() {
        let args = compile_args(&[
            MediaOp::SelectTracks(vec![0, 2]),
            MediaOp::SelectTracksByKind(vec![
                TrackKind::Video,
                TrackKind::Audio,
                TrackKind::Subtitle,
                TrackKind::Data,
                TrackKind::Attachment,
            ]),
        ]);

        for expected in ["0:0", "0:2", "0:v", "0:a", "0:s"] {
            assert!(
                args.windows(2).any(|w| w == ["-map", expected]),
                "missing {expected}: {args:?}"
            );
        }
        assert!(!args.windows(2).any(|w| w == ["-map", "0:d"]));
    }

    #[test]
    fn thumbnail_and_scene_detection_compile_to_ffmpeg_options() {
        let thumbnail = compile_args(&[MediaOp::GenerateThumbnail(ThumbnailConfig {
            timestamp: 2.5,
            width: Some(320),
            height: None,
            format: ImageFormat::Webp,
            quality: Some(80),
        })]);
        let ss_idx = thumbnail.iter().position(|a| a == "-ss").unwrap();
        assert!(thumbnail[ss_idx + 1].ends_with("2.500"));
        assert!(thumbnail.windows(2).any(|w| w == ["-vframes", "1"]));
        assert!(thumbnail.windows(2).any(|w| w == ["-c:v", "libwebp"]));
        assert!(thumbnail.windows(2).any(|w| w == ["-q:v", "80"]));
        assert!(thumbnail.windows(2).any(|w| w == ["-vf", "scale=320:-2"]));
        assert!(thumbnail.windows(2).any(|w| w == ["-f", "image2"]));

        let scenes = compile_args(&[MediaOp::DetectScenes(SceneDetectConfig {
            threshold: 0.42,
            min_scene_duration: 1.0,
            method: rskit_media::ops::SceneDetectMethod::ContentAware,
        })]);
        assert!(
            scenes
                .windows(2)
                .any(|w| w == ["-vf", "select='gt(scene,0.42)',showinfo"])
        );
        assert!(scenes.windows(2).any(|w| w == ["-f", "null"]));
    }

    #[test]
    fn visual_filter_presets_and_custom_adjustments_compile() {
        let bw = compile_args(&[MediaOp::ApplyFilter(FilterConfig {
            preset: FilterPreset::BW,
            intensity: 1.0,
            custom_params: None,
        })]);
        assert!(bw.windows(2).any(|w| w == ["-vf", "hue=s=0"]));

        let custom = compile_args(&[MediaOp::ApplyFilter(FilterConfig {
            preset: FilterPreset::Custom,
            intensity: 0.5,
            custom_params: Some(ColorAdjustments {
                brightness: Some(0.2),
                contrast: Some(0.4),
                saturation: Some(-0.2),
                temperature: None,
                gamma: Some(1.2),
            }),
        })]);
        assert!(custom.windows(2).any(|w| w
            == [
                "-vf",
                "eq=brightness=0.1:contrast=1.2:saturation=0.9:gamma=1.2"
            ]));
    }

    #[test]
    fn visual_filter_named_presets_compile() {
        for (preset, expected) in [
            (FilterPreset::Cinematic, "brightness=-0.05"),
            (FilterPreset::Warm, "brightness=0.05"),
            (FilterPreset::Cool, "brightness=-0.02"),
            (FilterPreset::Vintage, "saturation=0.7"),
            (FilterPreset::Dramatic, "contrast=1.3"),
        ] {
            let args = compile_args(&[MediaOp::ApplyFilter(FilterConfig {
                preset,
                intensity: 1.0,
                custom_params: None,
            })]);
            let vf_idx = args.iter().position(|arg| arg == "-vf").unwrap();
            assert!(
                args[vf_idx + 1].contains(expected),
                "missing {expected}: {args:?}"
            );
        }
    }

    #[test]
    fn custom_visual_filter_without_params_is_noop() {
        let args = compile_args(&[MediaOp::ApplyFilter(FilterConfig {
            preset: FilterPreset::Custom,
            intensity: 1.0,
            custom_params: None,
        })]);

        assert!(!args.contains(&"-vf".to_string()));
    }

    #[test]
    fn transcode_video_and_audio_options_compile() {
        let output = OutputConfig::new(Format::new("mp4"))
            .with_video(
                VideoSettings::new(Codec::new(video_codec::H264))
                    .with_resolution(Resolution::new(1920, 1080))
                    .with_frame_rate(FrameRate::ntsc_30())
                    .with_quality(Quality::High)
                    .with_bitrate(Bitrate::Constrained {
                        target: 2_000_000,
                        max: 3_000_000,
                    })
                    .with_speed(EncodingSpeed::VerySlow)
                    .with_profile(CodecProfile::H264High)
                    .with_level(CodecLevel::new("4.1")),
            )
            .with_audio(
                AudioSettings::new(Codec::new(audio_codec::AAC))
                    .with_sample_rate(SampleRate::dvd())
                    .with_channels(ChannelLayout::Stereo)
                    .with_bitrate(Bitrate::Variable(128_000)),
            )
            .with_strip_metadata()
            .with_param("movflags", "+faststart");

        let args = compile_args(&[MediaOp::Transcode(output)]);

        for expected in [
            ("-c:v", "libx264"),
            ("-crf", "18"),
            ("-b:v", "2000000"),
            ("-maxrate", "3000000"),
            ("-preset", "veryslow"),
            ("-r", "30000/1001"),
            ("-profile:v", "high"),
            ("-level", "4.1"),
            ("-c:a", "aac"),
            ("-ar", "48000"),
            ("-ac", "2"),
            ("-b:a", "128000"),
            ("-f", "mp4"),
            ("-map_metadata", "-1"),
            ("-movflags", "+faststart"),
        ] {
            assert!(
                args.windows(2).any(|w| w == [expected.0, expected.1]),
                "missing {expected:?}: {args:?}"
            );
        }
        assert!(args.windows(2).any(|w| w == ["-vf", "scale=1920:1080"]));
    }

    #[test]
    fn transcode_quality_bitrate_speed_and_streaming_variants_compile() {
        for (quality, crf) in [
            (Quality::Lossless, "0"),
            (Quality::UltraHigh, "14"),
            (Quality::Medium, "23"),
            (Quality::Low, "28"),
            (Quality::VeryLow, "35"),
            (Quality::Custom(31), "31"),
        ] {
            let output = OutputConfig::new(Format::new("mkv")).with_video(
                VideoSettings::new(Codec::new(video_codec::H265)).with_quality(quality),
            );
            let args = compile_args(&[MediaOp::Transcode(output)]);
            assert!(
                args.windows(2).any(|w| w == ["-crf", crf]),
                "missing {crf}: {args:?}"
            );
        }

        for (speed, preset) in [
            (EncodingSpeed::UltraFast, "ultrafast"),
            (EncodingSpeed::SuperFast, "superfast"),
            (EncodingSpeed::VeryFast, "veryfast"),
            (EncodingSpeed::Fast, "fast"),
            (EncodingSpeed::Medium, "medium"),
            (EncodingSpeed::Slow, "slow"),
        ] {
            let output = OutputConfig::new(Format::new("mp4"))
                .with_video(VideoSettings::new(Codec::new(video_codec::H264)).with_speed(speed));
            let args = compile_args(&[MediaOp::Transcode(output)]);
            assert!(
                args.windows(2).any(|w| w == ["-preset", preset]),
                "missing {preset}: {args:?}"
            );
        }

        let constant = compile_args(&[MediaOp::Transcode(
            OutputConfig::new(Format::new("mp4")).with_video(
                VideoSettings::new(Codec::new(video_codec::H264))
                    .with_bitrate(Bitrate::Constant(1_000_000)),
            ),
        )]);
        assert!(constant.windows(2).any(|w| w == ["-b:v", "1000000"]));

        let audio_constrained = compile_args(&[MediaOp::Transcode(
            OutputConfig::new(Format::new("mp3")).with_audio(
                AudioSettings::new(Codec::new(audio_codec::MP3)).with_bitrate(
                    Bitrate::Constrained {
                        target: 96_000,
                        max: 128_000,
                    },
                ),
            ),
        )]);
        assert!(audio_constrained.windows(2).any(|w| w == ["-b:a", "96000"]));

        let dash = compile_args(&[MediaOp::Transcode(
            OutputConfig::new(Format::new("mp4")).with_streaming(StreamingConfig::Dash(
                DashConfig {
                    segment_duration: 6,
                    use_template: true,
                    use_timeline: true,
                },
            )),
        )]);
        for expected in [
            ("-f", "dash"),
            ("-seg_duration", "6"),
            ("-use_template", "1"),
            ("-use_timeline", "1"),
        ] {
            assert!(
                dash.windows(2).any(|w| w == [expected.0, expected.1]),
                "missing {expected:?}: {dash:?}"
            );
        }

        let rtmp = compile_args(&[MediaOp::Transcode(
            OutputConfig::new(Format::new("flv")).with_streaming(StreamingConfig::Rtmp(
                RtmpConfig {
                    url: "rtmp://live.example.test/app/key".into(),
                },
            )),
        )]);
        assert!(rtmp.windows(2).any(|w| w == ["-f", "flv"]));
        assert!(rtmp.windows(2).any(|w| w == ["-rtmp_live", "live"]));
        assert!(rtmp.contains(&"rtmp://live.example.test/app/key".to_string()));
    }

    #[test]
    fn transcode_variable_video_bitrate_and_hls_event_options_compile() {
        let variable = compile_args(&[MediaOp::Transcode(
            OutputConfig::new(Format::new("mp4")).with_video(
                VideoSettings::new(Codec::new(video_codec::H264))
                    .with_bitrate(Bitrate::Variable(750_000)),
            ),
        )]);
        assert!(variable.windows(2).any(|w| w == ["-b:v", "750000"]));

        let hls = compile_args(&[MediaOp::Transcode(
            OutputConfig::new(Format::new("m3u8")).with_streaming(StreamingConfig::Hls(
                rskit_media::output::HlsConfig {
                    segment_duration: 4,
                    playlist_size: 3,
                    playlist_type: rskit_media::output::HlsPlaylistType::Event,
                    segment_filename: Some("seg-%03d.ts".into()),
                },
            )),
        )]);
        for expected in [
            ("-f", "hls"),
            ("-hls_time", "4"),
            ("-hls_list_size", "3"),
            ("-hls_playlist_type", "event"),
            ("-hls_segment_filename", "seg-%03d.ts"),
        ] {
            assert!(
                hls.windows(2).any(|w| w == [expected.0, expected.1]),
                "missing {expected:?}: {hls:?}"
            );
        }
    }

    #[test]
    fn burn_subtitles_operation_compiles_through_command_dispatch() {
        let subtitles = rskit_media::subtitle::SubtitleTrack::new()
            .add(TimeRange::from_seconds(0.0, 1.0), "hello");
        let args = compile_args(&[MediaOp::BurnSubtitles(subtitles)]);

        let vf_idx = args.iter().position(|arg| arg == "-vf").unwrap();
        assert!(args[vf_idx + 1].contains("subtitles=filename="));
    }

    #[test]
    fn add_subtitles_inline_content_compiles_to_temp_filter() {
        let args = compile_args(&[MediaOp::AddSubtitles(rskit_media::ops::SubtitleConfig {
            source: SubtitleSource::Inline("1\n00:00:00,000 --> 00:00:01,000\nhello\n".into()),
            format: SubtitleFormat::Srt,
            style: None,
        })]);

        let vf_idx = args.iter().position(|a| a == "-vf").unwrap();
        assert!(args[vf_idx + 1].starts_with("subtitles=filename="));
    }

    #[test]
    fn ai_operations_are_reported_as_unsupported_by_ffmpeg_backend() {
        let upscale = match FfmpegCommand::compile(
            &FileSource::from_path("/tmp/input.mp4"),
            &[MediaOp::Upscale(UpscaleConfig {
                model: UpscaleModel::RealEsrganX4Plus,
                scale: 4,
                denoise_strength: None,
            })],
            None,
            &default_config(),
            &default_registry(),
        ) {
            Ok(_) => panic!("upscale should be rejected by ffmpeg backend"),
            Err(error) => error,
        };
        assert_eq!(upscale.code(), rskit_errors::ErrorCode::InvalidInput);

        let interpolate = match FfmpegCommand::compile(
            &FileSource::from_path("/tmp/input.mp4"),
            &[MediaOp::Interpolate(InterpolateConfig {
                model: InterpolateModel::Rife,
                multiplier: 2,
            })],
            None,
            &default_config(),
            &default_registry(),
        ) {
            Ok(_) => panic!("interpolate should be rejected by ffmpeg backend"),
            Err(error) => error,
        };
        assert_eq!(interpolate.code(), rskit_errors::ErrorCode::InvalidInput);
    }
}
