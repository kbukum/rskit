//! Compilers for spatial video operations: Resize, Crop, Rotate, Flip, Pad.

use rskit_errors::AppResult;
use rskit_media::ops::{CropRegion, FlipDirection, PadOp, ResizeMode, ResizeOp, Rotation};

use super::CompileContext;

pub(crate) fn compile_resize(ctx: &mut CompileContext, op: &ResizeOp) -> AppResult<()> {
    let (w, h) = (op.resolution.width, op.resolution.height);
    let filter = match op.mode {
        ResizeMode::Exact => format!("scale={w}:{h}"),
        ResizeMode::Fit => format!(
            "scale={w}:{h}:force_original_aspect_ratio=decrease,pad={w}:{h}:(ow-iw)/2:(oh-ih)/2"
        ),
        ResizeMode::Fill => {
            format!("scale={w}:{h}:force_original_aspect_ratio=increase,crop={w}:{h}")
        }
        ResizeMode::FitWidth => format!("scale={w}:-2"),
        ResizeMode::FitHeight => format!("scale=-2:{h}"),
    };
    ctx.cmd.video_filters.push(filter);
    Ok(())
}

pub(crate) fn compile_crop(ctx: &mut CompileContext, region: &CropRegion) -> AppResult<()> {
    ctx.cmd.video_filters.push(format!(
        "crop={}:{}:{}:{}",
        region.width, region.height, region.x, region.y,
    ));
    Ok(())
}

pub(crate) fn compile_rotate(ctx: &mut CompileContext, rotation: &Rotation) -> AppResult<()> {
    let filter = match rotation {
        Rotation::Degrees90 => "transpose=1".to_string(),
        Rotation::Degrees180 => "hflip,vflip".to_string(),
        Rotation::Degrees270 => "transpose=2".to_string(),
        Rotation::Arbitrary(deg) => format!("rotate={deg}*PI/180"),
    };
    ctx.cmd.video_filters.push(filter);
    Ok(())
}

pub(crate) fn compile_flip(ctx: &mut CompileContext, dir: &FlipDirection) -> AppResult<()> {
    match dir {
        FlipDirection::Horizontal => ctx.cmd.video_filters.push("hflip".into()),
        FlipDirection::Vertical => ctx.cmd.video_filters.push("vflip".into()),
        FlipDirection::Both => {
            ctx.cmd.video_filters.push("hflip".into());
            ctx.cmd.video_filters.push("vflip".into());
        }
    }
    Ok(())
}

pub(crate) fn compile_pad(ctx: &mut CompileContext, pad: &PadOp) -> AppResult<()> {
    ctx.cmd.video_filters.push(format!(
        "pad={}:{}:(ow-iw)/2:(oh-ih)/2:{}",
        pad.width, pad.height, pad.color,
    ));
    Ok(())
}
