//! Compilers for temporal operations: Speed, Reverse.

use rskit_errors::AppResult;

use super::CompileContext;

pub(crate) fn compile_speed(ctx: &mut CompileContext, factor: f64) -> AppResult<()> {
    ctx.cmd.video_filters.push(format!("setpts=PTS/{factor}"));
    // FFmpeg atempo only supports 0.5–100.0 per filter
    let mut remaining = factor;
    while remaining > 2.0 {
        ctx.cmd.audio_filters.push("atempo=2.0".into());
        remaining /= 2.0;
    }
    while remaining < 0.5 {
        ctx.cmd.audio_filters.push("atempo=0.5".into());
        remaining /= 0.5;
    }
    ctx.cmd.audio_filters.push(format!("atempo={remaining}"));
    Ok(())
}

pub(crate) fn compile_reverse(ctx: &mut CompileContext) -> AppResult<()> {
    ctx.cmd.video_filters.push("reverse".into());
    ctx.cmd.audio_filters.push("areverse".into());
    Ok(())
}
