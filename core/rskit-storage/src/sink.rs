//! File sink — destination for file output.

use std::path::PathBuf;

use bytes::Bytes;
use rskit_errors::{AppError, AppResult, ErrorCode};
use rskit_fs::async_io::file;

use crate::{FileSource, TempFile};

/// Destination for file output.
pub enum FileSink {
    /// Write to a local path.
    Path(PathBuf),
    /// Write to a managed temp file (returned to caller).
    Temp,
    /// Write to an in-memory buffer.
    Memory,
}

impl FileSink {
    /// Create a writer handle for this sink.
    pub async fn writer(&self) -> AppResult<FileWriter> {
        match self {
            Self::Path(p) => {
                file::create_parent_dir(p).await?;
                Ok(FileWriter {
                    inner: WriterInner::Path(p.clone()),
                    buffer: Vec::new(),
                })
            }
            Self::Temp => Ok(FileWriter {
                inner: WriterInner::Temp,
                buffer: Vec::new(),
            }),
            Self::Memory => Ok(FileWriter {
                inner: WriterInner::Memory,
                buffer: Vec::new(),
            }),
        }
    }
}

impl std::fmt::Debug for FileSink {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Path(p) => f.debug_tuple("Path").field(p).finish(),
            Self::Temp => write!(f, "Temp"),
            Self::Memory => write!(f, "Memory"),
        }
    }
}

enum WriterInner {
    Path(PathBuf),
    Temp,
    Memory,
}

/// Handle for writing output. Finalize to get the resulting [`FileSource`].
pub struct FileWriter {
    inner: WriterInner,
    buffer: Vec<u8>,
}

impl FileWriter {
    /// Write all bytes to the output.
    pub async fn write_all(&mut self, data: &[u8]) -> AppResult<()> {
        self.buffer.extend_from_slice(data);
        Ok(())
    }

    /// Write a stream of bytes to the output.
    pub async fn write_stream(
        &mut self,
        mut stream: impl futures::Stream<Item = AppResult<Bytes>> + Unpin,
    ) -> AppResult<()> {
        use futures::StreamExt;
        while let Some(chunk) = stream.next().await {
            let chunk = chunk?;
            self.buffer.extend_from_slice(&chunk);
        }
        Ok(())
    }

    /// Finalize the writer and return the resulting [`FileSource`].
    pub async fn finalize(self) -> AppResult<FileSource> {
        match self.inner {
            WriterInner::Path(p) => {
                file::write(&p, &self.buffer).await.map_err(|e| {
                    AppError::new(
                        ErrorCode::Internal,
                        format!("failed to write {}: {e}", p.display()),
                    )
                })?;
                Ok(FileSource::Path(p))
            }
            WriterInner::Temp => {
                let tmp = TempFile::new()?;
                file::write(tmp.path(), &self.buffer).await.map_err(|e| {
                    AppError::new(
                        ErrorCode::Internal,
                        format!("failed to write temp file: {e}"),
                    )
                })?;
                Ok(FileSource::Temp(tmp))
            }
            WriterInner::Memory => Ok(FileSource::Bytes(Bytes::from(self.buffer))),
        }
    }
}
