use rskit_errors::AppResult;
use rskit_media::{ops::*, registry::Registry};
use rskit_storage::{FileSink, FileSource};

use crate::{compilers::CompileContext, config::FfmpegConfig};

use super::{FfmpegCommand, FfmpegInput, SourceHints};

impl FfmpegCommand {
    /// Compile a list of media operations into an FFmpeg command.
    ///
    /// Uses default [`SourceHints`] (assumes audio present).
    /// For more accurate compilation when stream info is known,
    /// use [`compile_with_hints`](Self::compile_with_hints).
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
}
