use rskit_errors::{AppError, AppResult, ErrorCode};
use rskit_storage::{FileSink, FileSource};

use super::retry::PreparedOutputPath;

/// based on the requested sink type.
pub(crate) async fn resolve_output(
    output: PreparedOutputPath,
    sink: Option<&FileSink>,
) -> AppResult<FileSource> {
    match sink {
        Some(FileSink::Path(_)) => Ok(FileSource::Path(output.path)),
        Some(FileSink::Memory) => {
            let data = tokio::fs::read(&output.path).await.map_err(|e| {
                AppError::new(ErrorCode::Internal, format!("read output failed: {e}"))
            })?;
            Ok(FileSource::Bytes(bytes::Bytes::from(data)))
        }
        _ => match output.temp {
            Some(temp) => Ok(temp.into_source()),
            None => Ok(FileSource::Path(output.path)),
        },
    }
}
