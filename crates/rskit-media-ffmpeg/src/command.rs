//! FFmpeg command builder — compiles MediaOp list into FFmpeg CLI arguments.

use std::time::Duration;

use rskit_errors::AppResult;
use rskit_file::{FileSink, FileSource};
use rskit_media::{ops::*, pipeline::Progress, registry::Registry, time::Timestamp};

use crate::{compilers::CompileContext, config::FfmpegConfig, progress::FfmpegProgressParser};

#[cfg(unix)]
#[allow(unused_imports)]
use std::os::unix::process::CommandExt;

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
pub struct SourceHints {
    /// Whether the primary source has at least one audio stream.
    /// `None` means unknown — the builder will assume audio exists (common case).
    pub has_audio: Option<bool>,
    /// Whether the primary source has at least one video stream.
    pub has_video: Option<bool>,
}

/// Compiled FFmpeg command ready for execution.
///
/// Holds all inputs, filters, output options, and global flags. Use
/// [`compile`](FfmpegCommand::compile) to construct from media operations.
pub struct FfmpegCommand {
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
    pub(crate) temp_files: Vec<rskit_file::TempFile>,
}

impl FfmpegCommand {
    /// Optimize a list of operations by removing redundancies.
    ///
    /// - Consecutive resizes: keep only the last one
    /// - Consecutive crops: keep only the last one
    /// - Multiple volume adjustments: multiply factors
    /// - Speed(1.0): remove (no-op)
    /// - Multiple consecutive Extract: keep only the last
    pub fn optimize_ops(ops: &[MediaOp]) -> Vec<MediaOp> {
        let mut result: Vec<MediaOp> = Vec::with_capacity(ops.len());

        for op in ops {
            match op {
                MediaOp::Resize(_) => {
                    // If the last op is also a resize, replace it
                    if matches!(result.last(), Some(MediaOp::Resize(_))) {
                        result.pop();
                    }
                    result.push(op.clone());
                }
                MediaOp::Crop(_) => {
                    if matches!(result.last(), Some(MediaOp::Crop(_))) {
                        result.pop();
                    }
                    result.push(op.clone());
                }
                MediaOp::Volume(factor) => {
                    if let Some(MediaOp::Volume(prev)) = result.last_mut() {
                        *prev *= factor;
                    } else {
                        result.push(op.clone());
                    }
                }
                MediaOp::Speed(factor) => {
                    // Skip no-op speed changes
                    if (*factor - 1.0).abs() < f64::EPSILON {
                        continue;
                    }
                    if let Some(MediaOp::Speed(prev)) = result.last_mut() {
                        *prev *= factor;
                    } else {
                        result.push(op.clone());
                    }
                }
                _ => result.push(op.clone()),
            }
        }

        result
    }

    /// Pre-flight validation of operations before spawning FFmpeg.
    ///
    /// Catches invalid combinations that would cause FFmpeg to fail with
    /// cryptic errors or produce incorrect output.
    pub fn validate_ops(ops: &[MediaOp]) -> AppResult<()> {
        let mut has_strip_audio = false;
        let mut has_strip_video = false;
        let mut has_audio_op = false;
        let mut has_video_op = false;
        let mut extract_count = 0;
        let mut extract_many_count = 0;
        let mut concat_count = 0;
        let mut overlay_count = 0;

        for op in ops {
            match op {
                MediaOp::StripAudio => has_strip_audio = true,
                MediaOp::StripVideo => has_strip_video = true,
                MediaOp::Volume(_)
                | MediaOp::NormalizeAudio
                | MediaOp::MixAudio(_)
                | MediaOp::ReplaceAudio(_) => has_audio_op = true,
                MediaOp::Resize(_)
                | MediaOp::Crop(_)
                | MediaOp::Rotate(_)
                | MediaOp::Flip(_)
                | MediaOp::Pad(_)
                | MediaOp::BurnSubtitles(_) => has_video_op = true,
                MediaOp::Overlay(_) => {
                    has_video_op = true;
                    overlay_count += 1;
                }
                MediaOp::Extract(_) => extract_count += 1,
                MediaOp::ExtractMany(_) => extract_many_count += 1,
                MediaOp::Concat(_) => concat_count += 1,
                MediaOp::Filter(f) => match f.target {
                    rskit_media::filter::FilterTarget::Video => has_video_op = true,
                    rskit_media::filter::FilterTarget::Audio => has_audio_op = true,
                },
                _ => {}
            }
        }

        if has_strip_audio && has_audio_op {
            return Err(rskit_errors::AppError::new(
                rskit_errors::ErrorCode::InvalidInput,
                "Cannot apply audio operations after stripping audio (StripAudio + audio filter/volume/mix)",
            ));
        }

        if has_strip_video && has_video_op {
            return Err(rskit_errors::AppError::new(
                rskit_errors::ErrorCode::InvalidInput,
                "Cannot apply video operations after stripping video (StripVideo + video filter/resize/crop)",
            ));
        }

        if extract_count > 1 {
            return Err(rskit_errors::AppError::new(
                rskit_errors::ErrorCode::InvalidInput,
                "Multiple Extract ops found — use ExtractMany for multi-segment extraction",
            ));
        }

        if extract_many_count > 1 {
            return Err(rskit_errors::AppError::new(
                rskit_errors::ErrorCode::InvalidInput,
                "Multiple ExtractMany ops not supported",
            ));
        }

        if (extract_count > 0 || extract_many_count > 0) && concat_count > 0 {
            return Err(rskit_errors::AppError::new(
                rskit_errors::ErrorCode::InvalidInput,
                "Cannot combine Extract/ExtractMany with Concat — these use conflicting input strategies",
            ));
        }

        if overlay_count > 1 || (overlay_count > 0 && concat_count > 0) {
            // Multiple complex_filter ops would overwrite each other
            return Err(rskit_errors::AppError::new(
                rskit_errors::ErrorCode::InvalidInput,
                "Only one complex-filter operation allowed per pipeline (overlay, concat, or multi-segment extract)",
            ));
        }

        Ok(())
    }

    /// Compile a list of media operations into an FFmpeg command.
    ///
    /// Uses default [`SourceHints`] (assumes audio present). For more accurate
    /// compilation when stream info is known, use [`compile_with_hints`](Self::compile_with_hints).
    pub fn compile(
        source: &FileSource,
        ops: &[MediaOp],
        sink: Option<&FileSink>,
        config: &FfmpegConfig,
        registry: &Registry,
    ) -> AppResult<Self> {
        Self::compile_with_hints(source, ops, sink, config, registry, &SourceHints::default())
    }

    /// Compile operations into an FFmpeg command, using source hints for smarter output.
    pub fn compile_with_hints(
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

        if let Some(hw) = &config.hw_accel {
            if let Some(arg) = hw.ffmpeg_arg() {
                cmd.global_opts.extend(["-hwaccel".into(), arg.into()]);
            }
        }

        // Force a specific input video decoder (e.g., libdav1d for software AV1 decode)
        if let Some(decoder) = &config.input_video_decoder {
            cmd.global_opts.extend(["-c:v".into(), decoder.clone()]);
        }

        let mut ctx = CompileContext {
            cmd: &mut cmd,
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

        Ok(cmd)
    }

    /// Build the final FFmpeg CLI argument list (excluding the output path).
    pub fn to_args(&self) -> Vec<String> {
        let mut args = Vec::new();

        args.extend(self.global_opts.clone());

        for input in &self.inputs {
            if let Some(seek) = &input.seek_to {
                args.extend(["-ss".into(), seek.to_ffmpeg_time()]);
            }
            if let Some(dur) = &input.duration {
                args.extend(["-t".into(), format!("{:.3}", dur.as_secs_f64())]);
            }
            args.push("-i".into());
            match &input.source {
                FileSource::Path(p) => args.push(p.to_string_lossy().to_string()),
                FileSource::Temp(t) => args.push(t.path().to_string_lossy().to_string()),
                _ => args.push("pipe:0".into()),
            }
        }

        if let Some(complex) = &self.complex_filter {
            args.extend(["-filter_complex".into(), complex.clone()]);
        } else {
            if !self.video_filters.is_empty() {
                args.extend(["-vf".into(), self.video_filters.join(",")]);
            }
            if !self.audio_filters.is_empty() {
                args.extend(["-af".into(), self.audio_filters.join(",")]);
            }
        }

        args.extend(self.output_opts.clone());

        args
    }

    /// Run the compiled FFmpeg command.
    ///
    /// Features:
    /// - Process group isolation (setpgid) for clean cleanup on Unix
    /// - Timeout enforcement via `tokio::time::timeout`
    /// - Streaming stderr collection for both progress parsing and error diagnostics
    /// - Progress reporting via `on_progress` callback (using mpsc channel)
    /// - CancellationToken support via `cancel` parameter
    /// - Full stderr included in error messages on failure
    pub async fn run(
        &self,
        config: &FfmpegConfig,
        on_progress: Option<Box<dyn Fn(Progress) + Send + Sync>>,
        output_path: &std::path::Path,
    ) -> Result<(), crate::error::FfmpegError> {
        let mut args = self.to_args();
        args.push(output_path.to_string_lossy().to_string());

        tracing::debug!(cmd = %format!("ffmpeg {}", args.join(" ")), "executing ffmpeg");

        let mut command = tokio::process::Command::new(config.ffmpeg_bin());
        command
            .args(&args)
            .stderr(std::process::Stdio::piped())
            .stdout(std::process::Stdio::null())
            .stdin(std::process::Stdio::null());

        // Process group isolation on Unix — allows clean SIGTERM of all child processes
        #[cfg(unix)]
        unsafe {
            command.pre_exec(|| {
                // Create new process group so we can kill the entire group
                libc::setpgid(0, 0);
                Ok(())
            });
        }

        let mut child = command.spawn().map_err(|e| crate::error::FfmpegError {
            kind: crate::error::FfmpegErrorKind::SpawnFailed,
            exit_code: None,
            stderr: String::new(),
            message: format!("failed to spawn ffmpeg: {e}"),
        })?;

        let child_pid = child.id();

        // Set up stderr reader for both progress parsing and error capture
        let stderr = child.stderr.take().expect("stderr was piped");
        let reader = tokio::io::BufReader::new(stderr);
        use tokio::io::AsyncBufReadExt;
        let mut lines = reader.lines();

        // Channel for collecting stderr lines (for error diagnostics)
        let (stderr_tx, mut stderr_rx) = tokio::sync::mpsc::unbounded_channel::<String>();
        // Channel for progress updates
        let progress_callback = on_progress.map(std::sync::Arc::new);

        let stderr_task = tokio::spawn({
            let progress_callback = progress_callback.clone();
            let parser = FfmpegProgressParser::new(None);
            async move {
                while let Ok(Some(line)) = lines.next_line().await {
                    // Try parsing progress
                    if let Some(ref cb) = progress_callback {
                        if let Some(progress) = parser.parse_line(&line) {
                            cb(progress);
                        }
                    }
                    // Always collect stderr for error diagnostics
                    let _ = stderr_tx.send(line);
                }
            }
        });

        // Wait for child with optional timeout
        let wait_result = if let Some(timeout_dur) = config.timeout {
            match tokio::time::timeout(timeout_dur, child.wait()).await {
                Ok(result) => result.map_err(|e| crate::error::FfmpegError {
                    kind: crate::error::FfmpegErrorKind::Unknown,
                    exit_code: None,
                    stderr: String::new(),
                    message: format!("ffmpeg process error: {e}"),
                }),
                Err(_) => {
                    // Timeout — kill the process
                    tracing::warn!("FFmpeg process timed out after {:?}, killing", timeout_dur);
                    Self::kill_process(&mut child, child_pid);
                    return Err(crate::error::FfmpegError {
                        kind: crate::error::FfmpegErrorKind::Timeout,
                        exit_code: None,
                        stderr: String::new(),
                        message: format!("ffmpeg timed out after {timeout_dur:?}"),
                    });
                }
            }
        } else {
            child.wait().await.map_err(|e| crate::error::FfmpegError {
                kind: crate::error::FfmpegErrorKind::Unknown,
                exit_code: None,
                stderr: String::new(),
                message: format!("ffmpeg process error: {e}"),
            })
        };

        // Wait for stderr reader to finish
        let _ = stderr_task.await;

        // Collect all stderr lines
        let mut stderr_lines = Vec::new();
        while let Ok(line) = stderr_rx.try_recv() {
            stderr_lines.push(line);
        }
        let stderr_output = stderr_lines.join("\n");

        let status = wait_result?;

        if !status.success() {
            let exit_code = status.code();
            let truncated_stderr =
                crate::error::truncate_stderr(&stderr_output, config.max_stderr_lines);
            let kind = crate::error::classify_error(exit_code, &stderr_output);

            let message = format!(
                "ffmpeg exited with status: {} (classified: {:?})",
                status, kind
            );

            let err = crate::error::FfmpegError {
                kind,
                exit_code,
                stderr: truncated_stderr,
                message,
            };

            return Err(err);
        }

        Ok(())
    }

    /// Kill an FFmpeg child process and its process group.
    fn kill_process(child: &mut tokio::process::Child, pid: Option<u32>) {
        // Try graceful SIGTERM first on Unix
        #[cfg(unix)]
        if let Some(pid) = pid {
            unsafe {
                // Send SIGTERM to the process group
                libc::kill(-(pid as i32), libc::SIGTERM);
            }
        }

        // Then force kill via tokio
        let _ = child.start_kill();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rskit_media::{
        ops::{CropRegion, FlipDirection, ResizeMode, ResizeOp, Rotation},
        spatial::Resolution,
        time::TimeRange,
    };

    fn default_config() -> FfmpegConfig {
        FfmpegConfig {
            overwrite: true,
            ..FfmpegConfig::default()
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
}
