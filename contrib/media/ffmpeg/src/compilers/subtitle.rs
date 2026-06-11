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
        SubtitleSource::File(path) => {
            let path = crate::paths::confine_source_path(ctx.config, path)?;
            read_subtitle_file(&path)?
        }
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
    let bytes = read_bounded(path, MAX_SUBTITLE_BYTES)?;
    String::from_utf8(bytes).map_err(|e| {
        rskit_errors::AppError::new(
            rskit_errors::ErrorCode::InvalidInput,
            format!("subtitle file is not valid UTF-8: {e}"),
        )
    })
}

fn read_bounded(path: &std::path::Path, max_bytes: u64) -> AppResult<Vec<u8>> {
    use std::io::Read as _;

    let mut file = std::fs::File::open(path).map_err(|e| {
        rskit_errors::AppError::new(
            rskit_errors::ErrorCode::Internal,
            format!("failed to open subtitle file: {e}"),
        )
    })?;
    let mut bytes = Vec::new();
    file.by_ref()
        .take(max_bytes + 1)
        .read_to_end(&mut bytes)
        .map_err(|e| {
            rskit_errors::AppError::new(
                rskit_errors::ErrorCode::Internal,
                format!("failed to read subtitle file: {e}"),
            )
        })?;
    if bytes.len() as u64 > max_bytes {
        return Err(rskit_errors::AppError::new(
            rskit_errors::ErrorCode::InvalidInput,
            format!("subtitle file exceeded max {max_bytes} bytes"),
        ));
    }

    Ok(bytes)
}

#[cfg(test)]
mod tests {
    use super::*;
    use rskit_media::{
        ops::{MediaOp, SubtitleConfig, SubtitleFormat, SubtitleSource},
        registry::Registry,
        subtitle::SubtitleTrack,
        time::TimeRange,
    };

    use crate::{command::FfmpegCommand, config::FfmpegConfig};

    #[test]
    fn add_subtitles_rejects_file_outside_configured_path_root() {
        let root = rskit_storage::TempDir::new().unwrap();
        let outside = rskit_storage::TempDir::new().unwrap();
        let subtitle = outside.path().join("captions.srt");
        std::fs::write(&subtitle, "1\n00:00:00,000 --> 00:00:01,000\nhello\n").unwrap();
        let config = FfmpegConfig::default().with_path_root(root.path());
        let op = MediaOp::AddSubtitles(SubtitleConfig {
            source: SubtitleSource::File(subtitle),
            format: SubtitleFormat::Srt,
            style: None,
        });
        let source = rskit_storage::FileSource::from_bytes(bytes::Bytes::from_static(b"media"));

        let result = FfmpegCommand::compile(&source, &[op], None, &config, &Registry::default());
        let error = match result {
            Ok(_) => panic!("outside subtitle path should be rejected"),
            Err(error) => error,
        };

        assert_eq!(error.code(), rskit_errors::ErrorCode::InvalidInput);
    }

    #[test]
    fn add_subtitles_accepts_file_inside_configured_path_root() {
        let root = rskit_storage::TempDir::new().unwrap();
        let subtitle = root.path().join("captions.srt");
        std::fs::write(&subtitle, "1\n00:00:00,000 --> 00:00:01,000\nhello\n").unwrap();
        let config = FfmpegConfig::default().with_path_root(root.path());
        let op = MediaOp::AddSubtitles(SubtitleConfig {
            source: SubtitleSource::File("captions.srt".into()),
            format: SubtitleFormat::Srt,
            style: None,
        });
        let source = rskit_storage::FileSource::from_bytes(bytes::Bytes::from_static(b"media"));

        FfmpegCommand::compile(&source, &[op], None, &config, &Registry::default()).unwrap();
    }

    #[test]
    fn burn_subtitles_writes_srt_filter_and_keeps_temp_file_alive() {
        let mut cmd = FfmpegCommand::compile(
            &rskit_storage::FileSource::from_bytes(bytes::Bytes::from_static(b"media")),
            &[],
            None,
            &FfmpegConfig::default(),
            &Registry::default(),
        )
        .unwrap();
        let config = FfmpegConfig::default();
        let registry = Registry::default();
        let hints = crate::command::SourceHints::default();
        let mut ctx = crate::compilers::CompileContext {
            cmd: &mut cmd,
            config: &config,
            hints: &hints,
            registry: &registry,
        };
        let subs = SubtitleTrack::new().add(TimeRange::from_seconds(0.0, 1.0), "hello");

        compile_burn_subtitles(&mut ctx, &subs).unwrap();

        assert_eq!(ctx.cmd.video_filters.len(), 1);
        assert!(ctx.cmd.video_filters[0].contains("subtitles=filename="));
        assert_eq!(ctx.cmd.temp_files.len(), 1);
        assert!(ctx.cmd.temp_files[0].path().exists());
    }

    #[test]
    fn subtitle_file_rejects_invalid_utf8() {
        let temp = rskit_storage::TempFile::with_extension("srt").unwrap();
        std::fs::write(temp.path(), [0xff, 0xfe, 0xfd]).unwrap();

        let error = read_subtitle_file(temp.path()).unwrap_err();

        assert_eq!(error.code(), rskit_errors::ErrorCode::InvalidInput);
    }

    #[test]
    fn subtitle_file_rejects_files_over_the_byte_limit() {
        let temp = rskit_storage::TempFile::with_extension("srt").unwrap();
        std::fs::write(temp.path(), vec![b'a'; (MAX_SUBTITLE_BYTES + 1) as usize]).unwrap();

        let error = read_subtitle_file(temp.path()).unwrap_err();

        assert_eq!(error.code(), rskit_errors::ErrorCode::InvalidInput);
    }

    #[test]
    fn subtitle_file_reports_open_errors() {
        let missing = std::path::Path::new("missing-captions.srt");

        let error = read_subtitle_file(missing).unwrap_err();

        assert_eq!(error.code(), rskit_errors::ErrorCode::Internal);
    }
}
