//! Compilers for audio operations: Volume, NormalizeAudio, FadeIn, FadeOut, StripAudio, StripVideo.

use std::time::Duration;

use rskit_errors::AppResult;

use super::CompileContext;

pub(crate) fn compile_volume(ctx: &mut CompileContext, factor: f64) -> AppResult<()> {
    ctx.cmd.audio_filters.push(format!("volume={factor}"));
    Ok(())
}

pub(crate) fn compile_normalize_audio(ctx: &mut CompileContext) -> AppResult<()> {
    ctx.cmd.audio_filters.push("loudnorm".into());
    Ok(())
}

pub(crate) fn compile_fade_in(ctx: &mut CompileContext, d: &Duration) -> AppResult<()> {
    let secs = d.as_secs_f64();
    ctx.cmd.video_filters.push(format!("fade=t=in:d={secs}"));
    ctx.cmd.audio_filters.push(format!("afade=t=in:d={secs}"));
    Ok(())
}

pub(crate) fn compile_fade_out(ctx: &mut CompileContext, d: &Duration) -> AppResult<()> {
    let secs = d.as_secs_f64();
    ctx.cmd.video_filters.push(format!("fade=t=out:d={secs}"));
    ctx.cmd.audio_filters.push(format!("afade=t=out:d={secs}"));
    Ok(())
}

pub(crate) fn compile_strip_audio(ctx: &mut CompileContext) -> AppResult<()> {
    ctx.cmd.output_opts.push("-an".into());
    Ok(())
}

pub(crate) fn compile_strip_video(ctx: &mut CompileContext) -> AppResult<()> {
    ctx.cmd.output_opts.push("-vn".into());
    Ok(())
}
