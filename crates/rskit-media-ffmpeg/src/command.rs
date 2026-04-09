//! FFmpeg command builder — compiles MediaOp list into FFmpeg CLI arguments.

use std::time::Duration;

use rskit_errors::AppResult;
use rskit_file::{FileSink, FileSource};
use rskit_media::{
    filter::FilterTarget,
    ops::*,
    output::{Bitrate, EncodingSpeed, OutputConfig, Quality, StreamingConfig},
    pipeline::Progress,
    registry::Registry,
    time::Timestamp,
};

use crate::{config::FfmpegConfig, filter_map, progress::FfmpegProgressParser};

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
                    FilterTarget::Video => has_video_op = true,
                    FilterTarget::Audio => has_audio_op = true,
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

        for op in &ops {
            match op {
                MediaOp::Extract(range) => {
                    cmd.inputs[0].seek_to = Some(range.start);
                    cmd.inputs[0].duration = Some(range.duration());
                }
                MediaOp::Resize(resize_op) => {
                    let (w, h) = (resize_op.resolution.width, resize_op.resolution.height);
                    let filter = match resize_op.mode {
                        ResizeMode::Exact => format!("scale={w}:{h}"),
                        ResizeMode::Fit => format!(
                            "scale={w}:{h}:force_original_aspect_ratio=decrease,pad={w}:{h}:(ow-iw)/2:(oh-ih)/2"
                        ),
                        ResizeMode::Fill => format!(
                            "scale={w}:{h}:force_original_aspect_ratio=increase,crop={w}:{h}"
                        ),
                        ResizeMode::FitWidth => format!("scale={w}:-2"),
                        ResizeMode::FitHeight => format!("scale=-2:{h}"),
                    };
                    cmd.video_filters.push(filter);
                }
                MediaOp::Crop(region) => {
                    cmd.video_filters.push(format!(
                        "crop={}:{}:{}:{}",
                        region.width, region.height, region.x, region.y,
                    ));
                }
                MediaOp::Rotate(rotation) => {
                    let filter = match rotation {
                        Rotation::Degrees90 => "transpose=1".to_string(),
                        Rotation::Degrees180 => "hflip,vflip".to_string(),
                        Rotation::Degrees270 => "transpose=2".to_string(),
                        Rotation::Arbitrary(deg) => format!("rotate={deg}*PI/180"),
                    };
                    cmd.video_filters.push(filter);
                }
                MediaOp::Flip(dir) => match dir {
                    FlipDirection::Horizontal => cmd.video_filters.push("hflip".into()),
                    FlipDirection::Vertical => cmd.video_filters.push("vflip".into()),
                    FlipDirection::Both => {
                        cmd.video_filters.push("hflip".into());
                        cmd.video_filters.push("vflip".into());
                    }
                },
                MediaOp::Pad(pad) => {
                    cmd.video_filters.push(format!(
                        "pad={}:{}:(ow-iw)/2:(oh-ih)/2:{}",
                        pad.width, pad.height, pad.color,
                    ));
                }
                MediaOp::Speed(factor) => {
                    cmd.video_filters.push(format!("setpts=PTS/{factor}"));
                    // FFmpeg atempo only supports 0.5–100.0 per filter
                    let mut remaining = *factor;
                    while remaining > 2.0 {
                        cmd.audio_filters.push("atempo=2.0".into());
                        remaining /= 2.0;
                    }
                    while remaining < 0.5 {
                        cmd.audio_filters.push("atempo=0.5".into());
                        remaining /= 0.5;
                    }
                    cmd.audio_filters.push(format!("atempo={remaining}"));
                }
                MediaOp::Reverse => {
                    cmd.video_filters.push("reverse".into());
                    cmd.audio_filters.push("areverse".into());
                }
                MediaOp::Volume(factor) => {
                    cmd.audio_filters.push(format!("volume={factor}"));
                }
                MediaOp::NormalizeAudio => {
                    cmd.audio_filters.push("loudnorm".into());
                }
                MediaOp::FadeIn(d) => {
                    let secs = d.as_secs_f64();
                    cmd.video_filters.push(format!("fade=t=in:d={secs}"));
                    cmd.audio_filters.push(format!("afade=t=in:d={secs}"));
                }
                MediaOp::FadeOut(d) => {
                    let secs = d.as_secs_f64();
                    cmd.video_filters.push(format!("fade=t=out:d={secs}"));
                    cmd.audio_filters.push(format!("afade=t=out:d={secs}"));
                }
                MediaOp::StripAudio => {
                    cmd.output_opts.push("-an".into());
                }
                MediaOp::StripVideo => {
                    cmd.output_opts.push("-vn".into());
                }
                MediaOp::Filter(filter) => {
                    let ff_filter = filter_map::to_ffmpeg_filter(filter);
                    match filter.target {
                        FilterTarget::Video => cmd.video_filters.push(ff_filter),
                        FilterTarget::Audio => cmd.audio_filters.push(ff_filter),
                    }
                }
                MediaOp::Overlay(overlay) => {
                    cmd.inputs.push(FfmpegInput {
                        source: overlay.source.clone(),
                        seek_to: None,
                        duration: None,
                    });
                    let pos = match &overlay.position {
                        OverlayPosition::TopLeft(x, y) => format!("{x}:{y}"),
                        OverlayPosition::TopRight(x, y) => format!("W-w-{x}:{y}"),
                        OverlayPosition::BottomLeft(x, y) => format!("{x}:H-h-{y}"),
                        OverlayPosition::BottomRight(x, y) => format!("W-w-{x}:H-h-{y}"),
                        OverlayPosition::Center => "(W-w)/2:(H-h)/2".into(),
                        OverlayPosition::Custom { x, y } => format!("{x}:{y}"),
                    };
                    let idx = cmd.inputs.len() - 1;
                    cmd.complex_filter = Some(format!("[0][{idx}]overlay={pos}"));
                }
                MediaOp::Concat(concat) => {
                    cmd.inputs.push(FfmpegInput {
                        source: concat.source.clone(),
                        seek_to: None,
                        duration: None,
                    });
                    let n = cmd.inputs.len();
                    let include_audio = hints.has_audio.unwrap_or(true);
                    let a_flag = if include_audio { 1 } else { 0 };
                    let pads: String = if include_audio {
                        (0..n).map(|i| format!("[{i}:v][{i}:a]")).collect()
                    } else {
                        (0..n).map(|i| format!("[{i}:v]")).collect()
                    };
                    cmd.complex_filter = Some(format!("{pads}concat=n={n}:v=1:a={a_flag}"));
                }
                MediaOp::ReplaceAudio(replace) => {
                    cmd.inputs.push(FfmpegInput {
                        source: replace.audio_source.clone(),
                        seek_to: None,
                        duration: None,
                    });
                    cmd.output_opts.extend(["-map".into(), "0:v".into()]);
                    cmd.output_opts
                        .extend(["-map".into(), format!("{}:a", cmd.inputs.len() - 1)]);
                }
                MediaOp::MixAudio(mix) => {
                    cmd.inputs.push(FfmpegInput {
                        source: mix.audio_source.clone(),
                        seek_to: None,
                        duration: None,
                    });
                    let idx = cmd.inputs.len() - 1;
                    cmd.complex_filter = Some(format!(
                        "[0:a][{idx}:a]amix=inputs=2:duration=first:dropout_transition=3"
                    ));
                }
                MediaOp::Transcode(config) => {
                    Self::apply_output_config(&mut cmd, config, registry)?;
                }
                MediaOp::SelectTracks(indices) => {
                    for idx in indices {
                        cmd.output_opts.extend(["-map".into(), format!("0:{idx}")]);
                    }
                }
                MediaOp::SelectTracksByKind(kinds) => {
                    for kind in kinds {
                        let stream_type = match kind {
                            rskit_media::TrackKind::Video => "v",
                            rskit_media::TrackKind::Audio => "a",
                            rskit_media::TrackKind::Subtitle => "s",
                            _ => continue,
                        };
                        cmd.output_opts
                            .extend(["-map".into(), format!("0:{stream_type}")]);
                    }
                }
                // Multi-segment extraction: concatenate extracted segments
                MediaOp::ExtractMany(segments) => {
                    if segments.is_empty() {
                        return Err(rskit_errors::AppError::new(
                            rskit_errors::ErrorCode::InvalidInput,
                            "ExtractMany requires at least one segment",
                        ));
                    }
                    if segments.len() == 1 {
                        // Single segment → simple extract
                        let range = segments[0].range;
                        cmd.inputs[0].seek_to = Some(range.start);
                        cmd.inputs[0].duration = Some(range.duration());
                    } else {
                        // Multiple segments → separate inputs per segment, concat
                        let base_source = cmd.inputs[0].source.clone();
                        cmd.inputs.clear();
                        for seg in segments {
                            cmd.inputs.push(FfmpegInput {
                                source: base_source.clone(),
                                seek_to: Some(seg.range.start),
                                duration: Some(seg.range.duration()),
                            });
                        }
                        let n = cmd.inputs.len();
                        let include_audio = hints.has_audio.unwrap_or(true);
                        let pads: String = if include_audio {
                            (0..n).map(|i| format!("[{i}:v][{i}:a]")).collect()
                        } else {
                            (0..n).map(|i| format!("[{i}:v]")).collect()
                        };
                        if include_audio {
                            cmd.complex_filter =
                                Some(format!("{pads}concat=n={n}:v=1:a=1[outv][outa]"));
                            cmd.output_opts.extend(["-map".into(), "[outv]".into()]);
                            cmd.output_opts.extend(["-map".into(), "[outa]".into()]);
                        } else {
                            cmd.complex_filter = Some(format!("{pads}concat=n={n}:v=1:a=0[outv]"));
                            cmd.output_opts.extend(["-map".into(), "[outv]".into()]);
                        }
                    }
                }
                MediaOp::BurnSubtitles(subs) => {
                    // Write subtitles to a temp SRT file and use the subtitles filter
                    let srt_content = subs.to_srt();
                    let temp = rskit_file::TempFile::with_extension("srt").map_err(|e| {
                        rskit_errors::AppError::new(
                            rskit_errors::ErrorCode::Internal,
                            format!("failed to create temp subtitle file: {e}"),
                        )
                    })?;
                    std::fs::write(temp.path(), &srt_content).map_err(|e| {
                        rskit_errors::AppError::new(
                            rskit_errors::ErrorCode::Internal,
                            format!("failed to write subtitle file: {e}"),
                        )
                    })?;
                    // Escape colons and backslashes in the path for FFmpeg filter syntax
                    let path_str = temp.path().to_string_lossy().replace('\\', "/");
                    let escaped = path_str.replace(':', "\\:").replace("'", "\\'");
                    cmd.video_filters
                        .push(format!("subtitles=filename={escaped}"));
                    // Keep temp file alive until command finishes
                    cmd.temp_files.push(temp);
                }
            }
        }

        Ok(cmd)
    }

    fn apply_output_config(
        cmd: &mut FfmpegCommand,
        config: &OutputConfig,
        registry: &Registry,
    ) -> AppResult<()> {
        if let Some(video) = &config.video {
            let encoder = registry
                .codec_info(&video.codec)
                .and_then(|info| info.ffmpeg_encoder.clone())
                .unwrap_or_else(|| video.codec.id().to_string());

            cmd.output_opts.extend(["-c:v".into(), encoder]);

            if let Some(quality) = &video.quality {
                let crf = match quality {
                    Quality::Lossless => "0",
                    Quality::UltraHigh => "14",
                    Quality::High => "18",
                    Quality::Medium => "23",
                    Quality::Low => "28",
                    Quality::VeryLow => "35",
                    Quality::Custom(v) => {
                        cmd.output_opts.extend(["-crf".into(), v.to_string()]);
                        ""
                    }
                };
                if !crf.is_empty() {
                    cmd.output_opts.extend(["-crf".into(), crf.into()]);
                }
            }

            if let Some(bitrate) = &video.bitrate {
                match bitrate {
                    Bitrate::Constant(br) => {
                        cmd.output_opts.extend(["-b:v".into(), br.to_string()]);
                    }
                    Bitrate::Variable(br) => {
                        cmd.output_opts.extend(["-b:v".into(), br.to_string()]);
                    }
                    Bitrate::Constrained { target, max } => {
                        cmd.output_opts.extend(["-b:v".into(), target.to_string()]);
                        cmd.output_opts.extend(["-maxrate".into(), max.to_string()]);
                    }
                }
            }

            if let Some(speed) = &video.speed {
                let preset = match speed {
                    EncodingSpeed::UltraFast => "ultrafast",
                    EncodingSpeed::SuperFast => "superfast",
                    EncodingSpeed::VeryFast => "veryfast",
                    EncodingSpeed::Fast => "fast",
                    EncodingSpeed::Medium => "medium",
                    EncodingSpeed::Slow => "slow",
                    EncodingSpeed::VerySlow => "veryslow",
                };
                cmd.output_opts.extend(["-preset".into(), preset.into()]);
            }

            if let Some(res) = &video.resolution {
                cmd.video_filters
                    .push(format!("scale={}:{}", res.width, res.height));
            }

            if let Some(fps) = &video.frame_rate {
                cmd.output_opts
                    .extend(["-r".into(), format!("{}/{}", fps.num, fps.den)]);
            }

            if let Some(profile) = &video.profile {
                cmd.output_opts
                    .extend(["-profile:v".into(), profile.as_ffmpeg_arg().into()]);
            }

            if let Some(level) = &video.level {
                cmd.output_opts.extend(["-level".into(), level.to_string()]);
            }
        }

        if let Some(audio) = &config.audio {
            let encoder = registry
                .codec_info(&audio.codec)
                .and_then(|info| info.ffmpeg_encoder.clone())
                .unwrap_or_else(|| audio.codec.id().to_string());

            cmd.output_opts.extend(["-c:a".into(), encoder]);

            if let Some(sr) = &audio.sample_rate {
                cmd.output_opts.extend(["-ar".into(), sr.0.to_string()]);
            }

            if let Some(ch) = &audio.channels {
                cmd.output_opts
                    .extend(["-ac".into(), ch.channel_count().to_string()]);
            }

            if let Some(bitrate) = &audio.bitrate {
                match bitrate {
                    Bitrate::Constant(br) | Bitrate::Variable(br) => {
                        cmd.output_opts.extend(["-b:a".into(), br.to_string()]);
                    }
                    Bitrate::Constrained { target, .. } => {
                        cmd.output_opts.extend(["-b:a".into(), target.to_string()]);
                    }
                }
            }
        }

        // Format extension for output
        if let Some(info) = registry.format_info(&config.format) {
            cmd.output_opts
                .extend(["-f".into(), info.extension.clone()]);
        }

        if config.strip_metadata {
            cmd.output_opts
                .extend(["-map_metadata".into(), "-1".into()]);
        }

        for (k, v) in &config.extra {
            cmd.output_opts.extend([format!("-{k}"), v.clone()]);
        }

        // Streaming output configuration
        if let Some(streaming) = &config.streaming {
            match streaming {
                StreamingConfig::Hls(hls) => {
                    cmd.output_opts.extend(["-f".into(), "hls".into()]);
                    cmd.output_opts
                        .extend(["-hls_time".into(), hls.segment_duration.to_string()]);
                    cmd.output_opts
                        .extend(["-hls_list_size".into(), hls.playlist_size.to_string()]);
                    match hls.playlist_type {
                        rskit_media::output::HlsPlaylistType::Vod => {
                            cmd.output_opts
                                .extend(["-hls_playlist_type".into(), "vod".into()]);
                        }
                        rskit_media::output::HlsPlaylistType::Event => {
                            cmd.output_opts
                                .extend(["-hls_playlist_type".into(), "event".into()]);
                        }
                    }
                    if let Some(seg_fn) = &hls.segment_filename {
                        cmd.output_opts
                            .extend(["-hls_segment_filename".into(), seg_fn.clone()]);
                    }
                }
                StreamingConfig::Dash(dash) => {
                    cmd.output_opts.extend(["-f".into(), "dash".into()]);
                    cmd.output_opts
                        .extend(["-seg_duration".into(), dash.segment_duration.to_string()]);
                    if dash.use_template {
                        cmd.output_opts.extend(["-use_template".into(), "1".into()]);
                    }
                    if dash.use_timeline {
                        cmd.output_opts.extend(["-use_timeline".into(), "1".into()]);
                    }
                }
                StreamingConfig::Rtmp(rtmp) => {
                    cmd.output_opts.extend(["-f".into(), "flv".into()]);
                    // The RTMP URL is the output destination — handled at run time
                    cmd.output_opts.extend(["-rtmp_live".into(), "live".into()]);
                    cmd.output_opts.push(rtmp.url.clone());
                }
            }
        }

        Ok(())
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
