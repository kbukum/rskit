//! FFmpeg command builder — compiles MediaOp list into FFmpeg CLI arguments.

use std::time::Duration;

use rskit_errors::{AppError, AppResult, ErrorCode};
use rskit_file::{FileSink, FileSource};
use rskit_media::{
    filter::FilterTarget,
    ops::*,
    output::{Bitrate, EncodingSpeed, OutputConfig, Quality},
    pipeline::Progress,
    registry::Registry,
    time::Timestamp,
};

use crate::{config::FfmpegConfig, filter_map, progress::FfmpegProgressParser};

pub(crate) struct FfmpegInput {
    pub source: FileSource,
    pub seek_to: Option<Timestamp>,
    pub duration: Option<Duration>,
}

pub(crate) struct FfmpegCommand {
    pub inputs: Vec<FfmpegInput>,
    pub video_filters: Vec<String>,
    pub audio_filters: Vec<String>,
    pub output_opts: Vec<String>,
    pub complex_filter: Option<String>,
    pub global_opts: Vec<String>,
}

impl FfmpegCommand {
    pub fn compile(
        source: &FileSource,
        ops: &[MediaOp],
        sink: Option<&FileSink>,
        config: &FfmpegConfig,
        registry: &Registry,
    ) -> AppResult<Self> {
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
        };

        if config.overwrite {
            cmd.global_opts.push("-y".into());
        }
        cmd.global_opts
            .extend(["-loglevel".into(), config.log_level.as_ffmpeg_arg().into()]);
        cmd.global_opts.extend(["-progress".into(), "pipe:2".into()]);

        if let Some(threads) = config.threads {
            cmd.global_opts
                .extend(["-threads".into(), threads.to_string()]);
        }

        if let Some(hw) = &config.hw_accel {
            cmd.global_opts
                .extend(["-hwaccel".into(), hw.ffmpeg_arg().into()]);
        }

        for op in ops {
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
                        Rotation::Arbitrary(deg) => format!("rotate={}*PI/180", deg),
                    };
                    cmd.video_filters.push(filter);
                }
                MediaOp::Flip(dir) => {
                    match dir {
                        FlipDirection::Horizontal => cmd.video_filters.push("hflip".into()),
                        FlipDirection::Vertical => cmd.video_filters.push("vflip".into()),
                        FlipDirection::Both => {
                            cmd.video_filters.push("hflip".into());
                            cmd.video_filters.push("vflip".into());
                        }
                    }
                }
                MediaOp::Pad(pad) => {
                    cmd.video_filters.push(format!(
                        "pad={}:{}:(ow-iw)/2:(oh-ih)/2:{}",
                        pad.width, pad.height, pad.color,
                    ));
                }
                MediaOp::Speed(factor) => {
                    cmd.video_filters
                        .push(format!("setpts=PTS/{factor}"));
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
                    let pads: String = (0..n).map(|i| format!("[{i}]")).collect();
                    cmd.complex_filter =
                        Some(format!("{pads}concat=n={n}:v=1:a=1"));
                }
                MediaOp::ReplaceAudio(replace) => {
                    cmd.inputs.push(FfmpegInput {
                        source: replace.audio_source.clone(),
                        seek_to: None,
                        duration: None,
                    });
                    cmd.output_opts.extend(["-map".into(), "0:v".into()]);
                    cmd.output_opts.extend([
                        "-map".into(),
                        format!("{}:a", cmd.inputs.len() - 1),
                    ]);
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
                        cmd.output_opts
                            .extend(["-map".into(), format!("0:{idx}")]);
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
                // Multi-segment extract and burn subtitles are more complex
                MediaOp::ExtractMany(_) | MediaOp::BurnSubtitles(_) => {
                    // TODO: implement multi-pass operations
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
                        cmd.output_opts
                            .extend(["-maxrate".into(), max.to_string()]);
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

        Ok(())
    }

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

    pub async fn run(
        &self,
        config: &FfmpegConfig,
        on_progress: Option<Box<dyn Fn(Progress) + Send + Sync>>,
        output_path: &std::path::Path,
    ) -> AppResult<()> {
        let mut args = self.to_args();
        args.push(output_path.to_string_lossy().to_string());

        tracing::debug!(cmd = %format!("ffmpeg {}", args.join(" ")), "executing ffmpeg");

        let mut child = tokio::process::Command::new(config.ffmpeg_bin())
            .args(&args)
            .stderr(std::process::Stdio::piped())
            .stdout(std::process::Stdio::null())
            .stdin(std::process::Stdio::null())
            .spawn()
            .map_err(|e| {
                AppError::new(ErrorCode::Internal, format!("failed to spawn ffmpeg: {e}"))
            })?;

        if let (Some(stderr), Some(_on_progress)) = (child.stderr.take(), &on_progress) {
            let _parser = FfmpegProgressParser::new(None);
            let reader = tokio::io::BufReader::new(stderr);
            use tokio::io::AsyncBufReadExt;
            let mut lines = reader.lines();

            tokio::spawn({
                let _on_progress_ptr = _on_progress as *const _ as usize;
                async move {
                    // SAFETY: The progress callback lives for the duration of the command
                    while let Ok(Some(line)) = lines.next_line().await {
                        let parser = FfmpegProgressParser::new(None);
                        if let Some(progress) = parser.parse_line(&line) {
                            // We can't safely use the callback across spawn without Arc
                            // This is simplified — in production, use channels
                            let _ = progress;
                        }
                    }
                }
            });
        }

        let status = child.wait().await.map_err(|e| {
            AppError::new(ErrorCode::Internal, format!("ffmpeg process error: {e}"))
        })?;

        if !status.success() {
            return Err(AppError::new(
                ErrorCode::Internal,
                format!("ffmpeg exited with status: {status}"),
            ));
        }

        Ok(())
    }
}
