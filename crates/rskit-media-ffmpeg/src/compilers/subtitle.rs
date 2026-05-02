//! Compilers for subtitle operations: BurnSubtitles, AddSubtitles.

use rskit_errors::AppResult;
use rskit_media::{ops::SubtitleConfig, subtitle::SubtitleTrack};

use super::CompileContext;

pub(crate) fn compile_burn_subtitles(
    ctx: &mut CompileContext,
    subs: &SubtitleTrack,
) -> AppResult<()> {
    let srt_content = subs.to_srt();
    let temp = rskit_storage::TempFile::with_extension("srt").map_err(|e| {
        rskit_errors::AppError::new(
            rskit_errors::ErrorCode::Internal,
            format!("failed to create temp subtitle file: {e}"),
        )
    })?;
    std::fs::write(temp.path(), &srt_content).map_err(|e| {
        rskit_errors::AppError::new(
            rskit_errors::ErrorCode::Internal,
            format!("failed to write subtitle file: {e}"),
        )
    })?;
    let path_str = temp.path().to_string_lossy().replace('\\', "/");
    let escaped = path_str.replace(':', "\\:").replace("'", "\\'");
    ctx.cmd
        .video_filters
        .push(format!("subtitles=filename={escaped}"));
    ctx.cmd.temp_files.push(temp);
    Ok(())
}

pub(crate) fn compile_add_subtitles(
    ctx: &mut CompileContext,
    cfg: &SubtitleConfig,
) -> AppResult<()> {
    use rskit_media::ops::SubtitleSource;
    let sub_content = match &cfg.source {
        SubtitleSource::File(path) => std::fs::read_to_string(path).map_err(|e| {
            rskit_errors::AppError::new(
                rskit_errors::ErrorCode::Internal,
                format!("failed to read subtitle file: {e}"),
            )
        })?,
        SubtitleSource::Inline(s) => s.clone(),
        _ => {
            return Err(rskit_errors::AppError::new(
                rskit_errors::ErrorCode::InvalidInput,
                "unsupported subtitle source",
            ));
        }
    };
    let ext = cfg.format.extension();
    let temp = rskit_storage::TempFile::with_extension(ext).map_err(|e| {
        rskit_errors::AppError::new(
            rskit_errors::ErrorCode::Internal,
            format!("failed to create temp subtitle file: {e}"),
        )
    })?;
    std::fs::write(temp.path(), &sub_content).map_err(|e| {
        rskit_errors::AppError::new(
            rskit_errors::ErrorCode::Internal,
            format!("failed to write subtitle file: {e}"),
        )
    })?;
    let path_str = temp.path().to_string_lossy().replace('\\', "/");
    let escaped = path_str.replace(':', "\\:").replace("'", "\\'");
    ctx.cmd
        .video_filters
        .push(format!("subtitles=filename={escaped}"));
    ctx.cmd.temp_files.push(temp);
    Ok(())
}
