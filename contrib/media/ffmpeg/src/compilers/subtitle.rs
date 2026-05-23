//! Compilers for subtitle operations: BurnSubtitles, AddSubtitles.

use rskit_errors::AppResult;
use rskit_media::{ops::SubtitleConfig, subtitle::SubtitleTrack};

use super::CompileContext;

const MAX_SUBTITLE_BYTES: u64 = 1024 * 1024;

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
        SubtitleSource::File(path) => read_subtitle_file(path)?,
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

fn read_subtitle_file(path: &std::path::Path) -> AppResult<String> {
    let size = std::fs::metadata(path)
        .map_err(|e| {
            rskit_errors::AppError::new(
                rskit_errors::ErrorCode::Internal,
                format!("failed to stat subtitle file: {e}"),
            )
        })?
        .len();
    if size > MAX_SUBTITLE_BYTES {
        return Err(rskit_errors::AppError::new(
            rskit_errors::ErrorCode::InvalidInput,
            format!("subtitle file is {size} bytes, exceeding max {MAX_SUBTITLE_BYTES}"),
        ));
    }
    let bytes = std::fs::read(path).map_err(|e| {
        rskit_errors::AppError::new(
            rskit_errors::ErrorCode::Internal,
            format!("failed to read subtitle file: {e}"),
        )
    })?;
    String::from_utf8(bytes).map_err(|e| {
        rskit_errors::AppError::new(
            rskit_errors::ErrorCode::InvalidInput,
            format!("subtitle file is not valid UTF-8: {e}"),
        )
    })
}
