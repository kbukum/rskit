use rskit_errors::ErrorCode;
use rskit_media::{
    executor::MediaExecutor, format::Format, ops::MediaOp, output::OutputConfig, registry::Registry,
};
use rskit_storage::{FileSink, FileSource, TempFile};
use tokio_util::sync::CancellationToken;

use super::{FfmpegExecutor, output::resolve_output, retry::PreparedOutputPath};
use crate::config::FfmpegConfig;

#[test]
fn test_config_input_video_decoder_default_is_none() {
    let config = FfmpegConfig::default();
    assert!(config.input_video_decoder.is_none());
}

#[test]
fn output_extension_uses_last_transcode_format_or_default() {
    let executor = FfmpegExecutor::new(FfmpegConfig::default(), Registry::default());
    let ops = [
        MediaOp::Transcode(OutputConfig::new(Format::new("mp4"))),
        MediaOp::Transcode(OutputConfig::new(Format::new("webm"))),
    ];

    assert_eq!(executor.determine_output_extension(&ops), "webm");
    assert_eq!(executor.determine_output_extension(&[]), "mkv");
}

#[tokio::test]
async fn resolve_output_maps_path_memory_and_temp_outputs() {
    let path_output = PreparedOutputPath {
        path: std::path::PathBuf::from("out.mkv"),
        is_user_path: true,
        temp: None,
    };
    let sink = FileSink::Path("out.mkv".into());
    assert!(matches!(
        resolve_output(path_output, Some(&sink)).await.unwrap(),
        FileSource::Path(_)
    ));

    let memory_file = TempFile::with_extension("mkv").unwrap();
    std::fs::write(memory_file.path(), b"media").unwrap();
    let memory_output = PreparedOutputPath {
        path: memory_file.path().to_path_buf(),
        is_user_path: false,
        temp: None,
    };
    assert!(matches!(
        resolve_output(memory_output, Some(&FileSink::Memory))
            .await
            .unwrap(),
        FileSource::Bytes(_)
    ));

    let temp = TempFile::with_extension("mkv").unwrap();
    let temp_output = PreparedOutputPath {
        path: temp.path().to_path_buf(),
        is_user_path: false,
        temp: Some(temp),
    };
    assert!(matches!(
        resolve_output(temp_output, None).await.unwrap(),
        FileSource::Temp(_)
    ));
}

#[tokio::test]
async fn cancellable_execution_returns_cancelled_before_spawning_ffmpeg() {
    let executor = FfmpegExecutor::new(FfmpegConfig::default(), Registry::default());
    let cancel = CancellationToken::new();
    cancel.cancel();

    let error = executor
        .execute_cancellable(
            &FileSource::from_bytes(bytes::Bytes::from_static(b"media")),
            &[],
            None,
            None,
            cancel,
        )
        .await
        .unwrap_err();

    assert_eq!(error.code(), ErrorCode::Cancelled);
}
