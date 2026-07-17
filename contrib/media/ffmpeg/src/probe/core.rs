use std::ffi::OsString;

use rskit_errors::{AppError, AppResult, ErrorCode};
use rskit_storage::FileSource;

use crate::config::FfmpegConfig;
use crate::process::{ensure_success, run_capture, with_context};

pub(crate) struct FfmpegProbe {
    pub(crate) config: FfmpegConfig,
}

impl FfmpegProbe {
    /// Create a new probe with the given configuration.
    pub(crate) fn new(config: FfmpegConfig) -> Self {
        Self { config }
    }

    /// Run ffprobe and return the raw JSON output.
    pub(crate) async fn probe_raw(&self, source: &FileSource) -> AppResult<serde_json::Value> {
        let resolved = source.to_local_path().await?;
        let path = crate::paths::resolved_source_path(&self.config, source, resolved.path())?;

        let output = run_capture(
            self.config.ffprobe_bin(),
            vec![
                OsString::from("-v"),
                OsString::from("quiet"),
                OsString::from("-print_format"),
                OsString::from("json"),
                OsString::from("-show_format"),
                OsString::from("-show_streams"),
                OsString::from("-show_chapters"),
                path.as_os_str().to_os_string(),
            ],
            self.config.timeout,
        )
        .await
        .map_err(|e| with_context(e, "ffprobe execution failed"))?;

        ensure_success(&output, "ffprobe")?;

        let json: serde_json::Value =
            serde_json::from_slice(&output.stdout_bytes).map_err(|e| {
                AppError::new(
                    ErrorCode::Internal,
                    format!("ffprobe output is not valid JSON: {e}"),
                )
            })?;

        Ok(json)
    }
}
