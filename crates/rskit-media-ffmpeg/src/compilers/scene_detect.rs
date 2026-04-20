//! Compiler for `DetectScenes`.

use rskit_errors::AppResult;
use rskit_media::ops::SceneDetectConfig;

use super::CompileContext;

pub(crate) fn compile_detect_scenes(
    ctx: &mut CompileContext,
    cfg: &SceneDetectConfig,
) -> AppResult<()> {
    ctx.cmd
        .video_filters
        .push(format!("select='gt(scene,{})',showinfo", cfg.threshold));
    ctx.cmd.output_opts.extend(["-f".into(), "null".into()]);
    Ok(())
}
