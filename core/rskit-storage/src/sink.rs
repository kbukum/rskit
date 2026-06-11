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

#[cfg(test)]
mod tests {
    use bytes::Bytes;
    use futures::stream;

    use super::*;

    #[test]
    fn file_sink_debug_describes_destination_without_side_effects() {
        assert_eq!(
            format!("{:?}", FileSink::Path(PathBuf::from("out.bin"))),
            "Path(\"out.bin\")"
        );
        assert_eq!(format!("{:?}", FileSink::Temp), "Temp");
        assert_eq!(format!("{:?}", FileSink::Memory), "Memory");
    }

    #[tokio::test]
    async fn memory_writer_collects_direct_and_streamed_bytes() {
        let mut writer = FileSink::Memory.writer().await.unwrap();
        writer.write_all(b"hello ").await.unwrap();
        writer
            .write_stream(stream::iter([
                Ok(Bytes::from_static(b"stream")),
                Ok(Bytes::from_static(b"!")),
            ]))
            .await
            .unwrap();

        let source = writer.finalize().await.unwrap();
        assert_eq!(
            source.read_all().await.unwrap(),
            Bytes::from_static(b"hello stream!")
        );
    }

    #[tokio::test]
    async fn path_and_temp_writers_materialize_file_sources() {
        let dir = tempfile::tempdir().unwrap();
        let output = dir.path().join("nested").join("file.txt");
        let mut path_writer = FileSink::Path(output.clone()).writer().await.unwrap();
        path_writer.write_all(b"path").await.unwrap();
        let path_source = path_writer.finalize().await.unwrap();
        assert!(matches!(path_source, FileSource::Path(_)));
        assert_eq!(tokio::fs::read(&output).await.unwrap(), b"path");

        let mut temp_writer = FileSink::Temp.writer().await.unwrap();
        temp_writer.write_all(b"temp").await.unwrap();
        let temp_source = temp_writer.finalize().await.unwrap();
        assert_eq!(
            temp_source.read_all().await.unwrap(),
            Bytes::from_static(b"temp")
        );
    }

    #[tokio::test]
    async fn writer_stream_forwards_chunk_errors() {
        let mut writer = FileSink::Memory.writer().await.unwrap();
        let error = AppError::new(ErrorCode::Internal, "chunk failed");
        let result = writer
            .write_stream(stream::iter([Err::<Bytes, _>(error)]))
            .await;
        assert_eq!(result.unwrap_err().code(), ErrorCode::Internal);
    }
}
