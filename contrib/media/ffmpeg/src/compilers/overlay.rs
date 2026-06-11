//! Compiler for `AddOverlay` (text/image overlay).

use rskit_errors::AppResult;
use rskit_media::ops::{OverlayConfig, OverlayType};
use rskit_storage::FileSource;

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

#[cfg(test)]
mod tests {
    use rskit_media::{
        ops::{MediaOp, OverlayConfig, OverlayType, Position, TextOverlay},
        registry::Registry,
        time::TimeRange,
    };

    use crate::{command::FfmpegCommand, config::FfmpegConfig};

    #[test]
    fn text_overlay_compiles_drawtext_with_escaping_and_time_range() {
        let source = rskit_storage::FileSource::from_bytes(bytes::Bytes::from_static(b"media"));
        let op = MediaOp::AddOverlay(OverlayConfig {
            overlay_type: OverlayType::Text(TextOverlay {
                text: "speaker: 'one'".into(),
                font_family: None,
                font_size: Some(32),
                color: Some("yellow".into()),
            }),
            position: Position { x: 0.25, y: 0.75 },
            size: None,
            opacity: 1.0,
            time_range: Some(TimeRange::from_seconds(1.0, 2.5)),
        });

        let cmd = FfmpegCommand::compile(
            &source,
            &[op],
            None,
            &FfmpegConfig::default(),
            &Registry::default(),
        )
        .unwrap();

        let filter = cmd.video_filters.join(",");
        assert!(filter.contains("drawtext=text='speaker\\: \\'one\\''"));
        assert!(filter.contains("fontsize=32"));
        assert!(filter.contains("fontcolor=yellow"));
        assert!(filter.contains(":x=(W*0.25):y=(H*0.75)"));
        assert!(filter.contains("enable='between(t,1,2.5)'"));
    }

    #[test]
    fn image_overlay_adds_secondary_input_and_complex_filter() {
        let source = rskit_storage::FileSource::from_bytes(bytes::Bytes::from_static(b"media"));
        let image = std::path::PathBuf::from("watermark.png");
        let op = MediaOp::AddOverlay(OverlayConfig {
            overlay_type: OverlayType::Image(image.clone()),
            position: Position { x: 0.1, y: 0.2 },
            size: None,
            opacity: 0.5,
            time_range: None,
        });

        let cmd = FfmpegCommand::compile(
            &source,
            &[op],
            None,
            &FfmpegConfig::default(),
            &Registry::default(),
        )
        .unwrap();

        assert_eq!(cmd.inputs.len(), 2);
        assert_eq!(
            cmd.complex_filter.as_deref(),
            Some("[0][1]overlay=(W*0.1):(H*0.2)")
        );
        match &cmd.inputs[1].source {
            rskit_storage::FileSource::Path(path) => assert_eq!(path, &image),
            other => panic!("expected image path input, got {other:?}"),
        }
    }
}
