//! Compilers for composition operations: Overlay, Concat, ReplaceAudio, MixAudio.

use rskit_errors::AppResult;
use rskit_media::ops::{ConcatOp, MixAudioOp, OverlayOp, OverlayPosition, ReplaceAudioOp};

use super::CompileContext;
use crate::command::FfmpegInput;

pub(crate) fn compile_overlay(ctx: &mut CompileContext, overlay: &OverlayOp) -> AppResult<()> {
    ctx.cmd.inputs.push(FfmpegInput {
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
    let idx = ctx.cmd.inputs.len() - 1;
    ctx.cmd.complex_filter = Some(format!("[0][{idx}]overlay={pos}"));
    Ok(())
}

pub(crate) fn compile_concat(ctx: &mut CompileContext, concat: &ConcatOp) -> AppResult<()> {
    ctx.cmd.inputs.push(FfmpegInput {
        source: concat.source.clone(),
        seek_to: None,
        duration: None,
    });
    let n = ctx.cmd.inputs.len();
    let include_audio = ctx.hints.has_audio.unwrap_or(true);
    let a_flag = if include_audio { 1 } else { 0 };
    let pads: String = if include_audio {
        (0..n).map(|i| format!("[{i}:v][{i}:a]")).collect()
    } else {
        (0..n).map(|i| format!("[{i}:v]")).collect()
    };
    ctx.cmd.complex_filter = Some(format!("{pads}concat=n={n}:v=1:a={a_flag}"));
    Ok(())
}

pub(crate) fn compile_replace_audio(
    ctx: &mut CompileContext,
    replace: &ReplaceAudioOp,
) -> AppResult<()> {
    ctx.cmd.inputs.push(FfmpegInput {
        source: replace.audio_source.clone(),
        seek_to: None,
        duration: None,
    });
    ctx.cmd.output_opts.extend(["-map".into(), "0:v".into()]);
    ctx.cmd
        .output_opts
        .extend(["-map".into(), format!("{}:a", ctx.cmd.inputs.len() - 1)]);
    Ok(())
}

pub(crate) fn compile_mix_audio(ctx: &mut CompileContext, mix: &MixAudioOp) -> AppResult<()> {
    ctx.cmd.inputs.push(FfmpegInput {
        source: mix.audio_source.clone(),
        seek_to: None,
        duration: None,
    });
    let idx = ctx.cmd.inputs.len() - 1;
    ctx.cmd.complex_filter = Some(format!(
        "[0:a][{idx}:a]amix=inputs=2:duration=first:dropout_transition=3"
    ));
    Ok(())
}
