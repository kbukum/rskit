//! Compilers for `Extract` and `ExtractMany` operations.

use rskit_errors::AppResult;
use rskit_media::time::{Segment, TimeRange};

use super::CompileContext;
use crate::command::FfmpegInput;

pub(crate) fn compile_extract(ctx: &mut CompileContext, range: &TimeRange) -> AppResult<()> {
    ctx.cmd.inputs[0].seek_to = Some(range.start);
    ctx.cmd.inputs[0].duration = Some(range.duration());
    Ok(())
}

pub(crate) fn compile_extract_many(
    ctx: &mut CompileContext,
    segments: &[Segment],
) -> AppResult<()> {
    if segments.is_empty() {
        return Err(rskit_errors::AppError::new(
            rskit_errors::ErrorCode::InvalidInput,
            "ExtractMany requires at least one segment",
        ));
    }
    if segments.len() == 1 {
        let range = segments[0].range;
        ctx.cmd.inputs[0].seek_to = Some(range.start);
        ctx.cmd.inputs[0].duration = Some(range.duration());
    } else {
        let base_source = ctx.cmd.inputs[0].source.clone();
        ctx.cmd.inputs.clear();
        for seg in segments {
            ctx.cmd.inputs.push(FfmpegInput {
                source: base_source.clone(),
                seek_to: Some(seg.range.start),
                duration: Some(seg.range.duration()),
            });
        }
        let n = ctx.cmd.inputs.len();
        let include_audio = ctx.hints.has_audio.unwrap_or(true);
        let pads: String = if include_audio {
            (0..n).map(|i| format!("[{i}:v][{i}:a]")).collect()
        } else {
            (0..n).map(|i| format!("[{i}:v]")).collect()
        };
        if include_audio {
            ctx.cmd.complex_filter = Some(format!("{pads}concat=n={n}:v=1:a=1[outv][outa]"));
            ctx.cmd.output_opts.extend(["-map".into(), "[outv]".into()]);
            ctx.cmd.output_opts.extend(["-map".into(), "[outa]".into()]);
        } else {
            ctx.cmd.complex_filter = Some(format!("{pads}concat=n={n}:v=1:a=0[outv]"));
            ctx.cmd.output_opts.extend(["-map".into(), "[outv]".into()]);
        }
    }
    Ok(())
}
