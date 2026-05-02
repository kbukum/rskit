//! Compiler for `AddOverlay` (text/image overlay).

use rskit_errors::AppResult;
use rskit_storage::FileSource;
use rskit_media::ops::{OverlayConfig, OverlayType};

use super::CompileContext;
use crate::command::FfmpegInput;

pub(crate) fn compile_add_overlay(ctx: &mut CompileContext, cfg: &OverlayConfig) -> AppResult<()> {
    match &cfg.overlay_type {
        OverlayType::Text(text_cfg) => {
            let fontsize = text_cfg.font_size.unwrap_or(24);
            let color = text_cfg.color.as_deref().unwrap_or("white");
            let x = format!("(W*{})", cfg.position.x);
            let y = format!("(H*{})", cfg.position.y);
            let escaped_text = text_cfg.text.replace(':', "\\:").replace("'", "\\'");
            let mut filter = format!(
                "drawtext=text='{escaped_text}':fontsize={fontsize}\
                 :fontcolor={color}:x={x}:y={y}",
            );
            if let Some(range) = &cfg.time_range {
                let start = range.start.as_seconds();
                let end = range.end.as_seconds();
                filter.push_str(&format!(":enable='between(t,{start},{end})'"));
            }
            ctx.cmd.video_filters.push(filter);
        }
        OverlayType::Image(path) => {
            ctx.cmd.inputs.push(FfmpegInput {
                source: FileSource::Path(path.clone()),
                seek_to: None,
                duration: None,
            });
            let idx = ctx.cmd.inputs.len() - 1;
            let x = format!("(W*{})", cfg.position.x);
            let y = format!("(H*{})", cfg.position.y);
            ctx.cmd.complex_filter = Some(format!("[0][{idx}]overlay={x}:{y}"));
        }
        _ => {
            return Err(rskit_errors::AppError::new(
                rskit_errors::ErrorCode::InvalidInput,
                "unsupported overlay type",
            ));
        }
    }
    Ok(())
}
