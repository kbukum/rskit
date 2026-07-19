//! Duration-aware timeout resolution and source introspection.
//!
//! Provides helpers used by [`FfmpegExecutor`] before executing commands:
//! - [`infer_operation_kind`] — classify a `MediaOp` list into an [`OperationKind`]
//! - [`FfmpegExecutor::resolve_effective_config`] — probe duration + compute timeout
//! - [`FfmpegExecutor::quick_probe_duration`] — lightweight ffprobe for duration only
//! - [`FfmpegExecutor::build_source_hints`] — detect audio stream presence for hints

use std::ffi::OsString;
use std::time::Duration;

use rskit_errors::{AppResult, ErrorCode};
use rskit_media::ops::MediaOp;
use rskit_media::timeout::OperationKind;
use rskit_storage::FileSource;

use crate::command::SourceHints;
use crate::config::FfmpegConfig;
use crate::process::run_capture_with_cancel;
use tokio_util::sync::CancellationToken;

use super::FfmpegExecutor;

/// Infer the dominant [`OperationKind`] from a list of media operations.
///
/// Picks the "heaviest" operation kind when multiple ops are present,
/// since the total timeout should be driven by the most expensive step.
#[must_use]
pub(crate) fn infer_operation_kind(ops: &[MediaOp]) -> OperationKind {
    let mut heaviest = OperationKind::StreamCopy;

    for op in ops {
        let kind = op.timeout_kind();

        if kind.default_multiplier() > heaviest.default_multiplier() {
            heaviest = kind;
        }
    }

    heaviest
}

impl FfmpegExecutor {
    /// Create an effective config with duration-aware timeout resolved.
    ///
    /// When the config has a [`rskit_media::timeout::TimeoutCalculator`], this probes the source duration
    /// and infers the operation kind to compute a scaled timeout,
    /// replacing the fixed `timeout` field in the returned config.
    pub(crate) async fn resolve_effective_config(
        &self,
        source: &FileSource,
        ops: &[MediaOp],
        cancel: CancellationToken,
    ) -> AppResult<FfmpegConfig> {
        // If no calculator is configured, skip the probe entirely.
        if self.config.timeout_calculator.is_none() {
            return Ok(self.config.clone());
        }

        let source_duration = self.quick_probe_duration(source, cancel).await?;
        let op_kind = infer_operation_kind(ops);

        if let Some(resolved) = self.config.resolve_timeout(source_duration, Some(op_kind)) {
            tracing::debug!(
                source_duration_secs = source_duration.map(|d| d.as_secs()),
                op_kind = ?op_kind,
                resolved_timeout_secs = resolved.as_secs(),
                "resolved duration-aware timeout"
            );
            let mut cfg = self.config.clone();
            cfg.timeout = Some(resolved);
            Ok(cfg)
        } else {
            Ok(self.config.clone())
        }
    }

    /// Quick ffprobe to get source duration (for timeout calculation).
    pub(crate) async fn quick_probe_duration(
        &self,
        source: &FileSource,
        cancel: CancellationToken,
    ) -> AppResult<Option<Duration>> {
        let path = match source {
            FileSource::Path(p) => crate::paths::confine_source_path(&self.config, p)?,
            _ => return Ok(None),
        };

        let output = run_capture_with_cancel(
            self.config.ffprobe_bin(),
            vec![
                OsString::from("-v"),
                OsString::from("quiet"),
                OsString::from("-show_entries"),
                OsString::from("format=duration"),
                OsString::from("-of"),
                OsString::from("csv=p=0"),
                path.as_os_str().to_os_string(),
            ],
            self.config.timeout,
            cancel,
        )
        .await
        .map_err(|error| {
            if error.code() == ErrorCode::Cancelled {
                error
            } else {
                rskit_errors::AppError::new(
                    ErrorCode::Internal,
                    format!("failed to probe source duration: {error}"),
                )
            }
        })?;

        let Some(secs) = output.stdout.trim().parse::<f64>().ok() else {
            return Ok(None);
        };
        Ok(Some(Duration::from_secs_f64(secs)))
    }

    /// Build source hints by quick-probing when concat/extract-many ops need stream info.
    pub(crate) async fn build_source_hints(
        &self,
        source: &FileSource,
        ops: &[MediaOp],
        cancel: CancellationToken,
    ) -> AppResult<SourceHints> {
        let needs_hints = ops.iter().any(MediaOp::needs_stream_hints);
        if !needs_hints {
            return Ok(SourceHints::default());
        }

        // Quick ffprobe to detect whether audio pads should be included.
        let path = match source {
            FileSource::Path(p) => crate::paths::confine_source_path(&self.config, p)?,
            _ => return Ok(SourceHints::default()),
        };

        let output = run_capture_with_cancel(
            self.config.ffprobe_bin(),
            vec![
                OsString::from("-v"),
                OsString::from("quiet"),
                OsString::from("-show_entries"),
                OsString::from("stream=codec_type"),
                OsString::from("-of"),
                OsString::from("csv=p=0"),
                path.as_os_str().to_os_string(),
            ],
            self.config.timeout,
            cancel,
        )
        .await;

        match output {
            Ok(out) => {
                let has_audio = out.stdout.lines().any(|l| l.trim() == "audio");
                Ok(SourceHints {
                    has_audio: Some(has_audio),
                })
            }
            Err(error) if error.code() == ErrorCode::Cancelled => Err(error),
            Err(_) => Ok(SourceHints::default()),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rskit_media::{
        ops::{ConcatOp, MediaOp, ResizeMode, ResizeOp},
        registry::Registry,
        spatial::Resolution,
        timeout::TimeoutCalculator,
    };

    #[cfg(unix)]
    use crate::test_support::write_executable_script as write_script;

    fn executor(config: FfmpegConfig) -> FfmpegExecutor {
        FfmpegExecutor::new(config, Registry::default())
    }

    #[test]
    fn infer_operation_kind_uses_heaviest_operation() {
        let ops = vec![
            MediaOp::StripAudio,
            MediaOp::Resize(ResizeOp {
                resolution: Resolution::new(320, 240),
                mode: ResizeMode::Exact,
            }),
        ];

        assert_eq!(infer_operation_kind(&ops), OperationKind::Transcode);
        assert_eq!(infer_operation_kind(&[]), OperationKind::StreamCopy);
    }

    #[tokio::test]
    async fn effective_config_skips_probe_without_timeout_calculator() {
        let exec = executor(FfmpegConfig::default().with_timeout(Duration::from_secs(9)));

        let config = exec
            .resolve_effective_config(
                &FileSource::from_bytes(bytes::Bytes::from_static(b"media")),
                &[],
                CancellationToken::new(),
            )
            .await
            .unwrap();

        assert_eq!(config.timeout, Some(Duration::from_secs(9)));
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn effective_config_uses_probed_duration_when_available() {
        let ffprobe = write_script("printf '10.0'");
        let input = rskit_storage::TempFile::with_extension("mp4").unwrap();
        std::fs::write(input.path(), b"media").unwrap();
        let config = FfmpegConfig::default()
            .with_ffprobe_path(ffprobe.path())
            .with_timeout_calculator(
                TimeoutCalculator::default()
                    .with_base_timeout(Duration::from_secs(10))
                    .with_max_timeout(Duration::from_secs(1_000))
                    .with_multiplier(OperationKind::Filter, 1.0),
            );
        let exec = executor(config);

        let config = exec
            .resolve_effective_config(
                &FileSource::from_path(input.path()),
                &[MediaOp::Resize(ResizeOp {
                    resolution: Resolution::new(320, 240),
                    mode: ResizeMode::Exact,
                })],
                CancellationToken::new(),
            )
            .await
            .unwrap();

        assert_eq!(config.timeout, Some(Duration::from_secs(145)));
    }

    #[tokio::test]
    async fn quick_probe_duration_skips_non_path_sources() {
        let exec = executor(FfmpegConfig::default());

        let duration = exec
            .quick_probe_duration(
                &FileSource::from_bytes(bytes::Bytes::from_static(b"media")),
                CancellationToken::new(),
            )
            .await
            .unwrap();

        assert_eq!(duration, None);
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn quick_probe_duration_returns_none_for_unparseable_stdout() {
        let ffprobe = write_script("printf 'N/A'");
        let input = rskit_storage::TempFile::with_extension("mp4").unwrap();
        std::fs::write(input.path(), b"media").unwrap();
        let exec = executor(FfmpegConfig::default().with_ffprobe_path(ffprobe.path()));

        let duration = exec
            .quick_probe_duration(
                &FileSource::from_path(input.path()),
                CancellationToken::new(),
            )
            .await
            .unwrap();

        assert_eq!(duration, None);
    }

    #[tokio::test]
    async fn source_hints_skip_probe_when_operations_do_not_need_stream_info() {
        let exec = executor(FfmpegConfig::default());

        let hints = exec
            .build_source_hints(
                &FileSource::from_bytes(bytes::Bytes::from_static(b"media")),
                &[MediaOp::StripAudio],
                CancellationToken::new(),
            )
            .await
            .unwrap();

        assert_eq!(hints.has_audio, None);
    }

    #[tokio::test]
    async fn source_hints_skip_non_path_sources_even_when_needed() {
        let exec = executor(FfmpegConfig::default());
        let ops = [MediaOp::Concat(ConcatOp {
            source: FileSource::from_path("/tmp/second.mp4"),
            transition: None,
        })];

        let hints = exec
            .build_source_hints(
                &FileSource::from_bytes(bytes::Bytes::from_static(b"media")),
                &ops,
                CancellationToken::new(),
            )
            .await
            .unwrap();

        assert_eq!(hints.has_audio, None);
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn source_hints_detect_audio_streams_from_ffprobe_stdout() {
        let ffprobe = write_script("printf 'video\\naudio\\n'");
        let input = rskit_storage::TempFile::with_extension("mp4").unwrap();
        std::fs::write(input.path(), b"media").unwrap();
        let exec = executor(FfmpegConfig::default().with_ffprobe_path(ffprobe.path()));
        let ops = [MediaOp::Concat(ConcatOp {
            source: FileSource::from_path("/tmp/second.mp4"),
            transition: None,
        })];

        let hints = exec
            .build_source_hints(
                &FileSource::from_path(input.path()),
                &ops,
                CancellationToken::new(),
            )
            .await
            .unwrap();

        assert_eq!(hints.has_audio, Some(true));
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn source_hints_report_no_audio_when_probe_stdout_has_no_audio() {
        let ffprobe = write_script("exit 3");
        let input = rskit_storage::TempFile::with_extension("mp4").unwrap();
        std::fs::write(input.path(), b"media").unwrap();
        let exec = executor(FfmpegConfig::default().with_ffprobe_path(ffprobe.path()));
        let ops = [MediaOp::Concat(ConcatOp {
            source: FileSource::from_path("/tmp/second.mp4"),
            transition: None,
        })];

        let hints = exec
            .build_source_hints(
                &FileSource::from_path(input.path()),
                &ops,
                CancellationToken::new(),
            )
            .await
            .unwrap();

        assert_eq!(hints.has_audio, Some(false));
    }
}
