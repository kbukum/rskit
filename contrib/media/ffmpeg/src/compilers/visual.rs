//! Compiler for `ApplyFilter` (color grading presets + custom adjustments).

use rskit_errors::AppResult;
use rskit_media::ops::{FilterConfig, FilterPreset};

use super::CompileContext;

pub(crate) fn compile_apply_filter(ctx: &mut CompileContext, cfg: &FilterConfig) -> AppResult<()> {
    let i = cfg.intensity;
    match cfg.preset {
        FilterPreset::BW => {
            ctx.cmd.video_filters.push("hue=s=0".to_string());
        }
        FilterPreset::Custom => {
            if let Some(adj) = &cfg.custom_params {
                let mut parts = Vec::new();
                if let Some(b) = adj.brightness {
                    parts.push(format!("brightness={}", b * i));
                }
                if let Some(c) = adj.contrast {
                    parts.push(format!("contrast={}", 1.0 + c * i));
                }
                if let Some(s) = adj.saturation {
                    parts.push(format!("saturation={}", 1.0 + s * i));
                }
                if let Some(g) = adj.gamma {
                    parts.push(format!("gamma={g}"));
                }
                if !parts.is_empty() {
                    ctx.cmd
                        .video_filters
                        .push(format!("eq={}", parts.join(":")));
                }
            }
        }
        preset => {
            let (br, con, sat) = match preset {
                FilterPreset::Cinematic => (-0.05, 1.1, 0.9),
                FilterPreset::Warm => (0.05, 1.0, 1.1),
                FilterPreset::Cool => (-0.02, 1.0, 0.95),
                FilterPreset::Vintage => (0.04, 0.9, 0.7),
                FilterPreset::Dramatic => (-0.1, 1.3, 1.1),
                _ => (0.0, 1.0, 1.0),
            };
            ctx.cmd.video_filters.push(format!(
                "eq=brightness={}:contrast={}:saturation={}",
                br * i,
                1.0 + (con - 1.0) * i,
                1.0 + (sat - 1.0) * i,
            ));
        }
    }
    Ok(())
}
