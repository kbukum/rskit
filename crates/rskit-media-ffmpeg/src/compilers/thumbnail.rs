//! Compiler for `GenerateThumbnail`.

use rskit_errors::AppResult;
use rskit_media::{ops::ThumbnailConfig, time::Timestamp};

use super::CompileContext;

pub(crate) fn compile_generate_thumbnail(
    ctx: &mut CompileContext,
    cfg: &ThumbnailConfig,
) -> AppResult<()> {
    ctx.cmd.inputs[0].seek_to = Some(Timestamp::from_seconds(cfg.timestamp));
    ctx.cmd.output_opts.extend(["-vframes".into(), "1".into()]);
    ctx.cmd
        .output_opts
        .extend(["-c:v".into(), cfg.format.ffmpeg_codec().into()]);
    if let Some(q) = cfg.quality {
        ctx.cmd.output_opts.extend(["-q:v".into(), q.to_string()]);
    }
    if let Some(w) = cfg.width {
        let h = cfg.height.map_or(-2i32, |v| v as i32);
        ctx.cmd.video_filters.push(format!("scale={w}:{h}"));
    } else if let Some(h) = cfg.height {
        ctx.cmd.video_filters.push(format!("scale=-2:{h}"));
    }
    ctx.cmd.output_opts.extend(["-f".into(), "image2".into()]);
    Ok(())
}
