//! File source — a reference to file content that can be read.

use std::path::{Path, PathBuf};

use bytes::Bytes;
use futures::Stream;
use rskit_errors::{AppError, AppResult, ErrorCode};
use rskit_fs::async_io::file;
use tokio::io::AsyncRead;

use crate::TempFile;

/// A reference to file content that can be read. Does NOT load content eagerly —
/// all reads are lazy/streamed.
///
/// Serialization notes:
/// - `Temp` serializes as `Path` (temp file's path).
/// - `Bytes` serializes as a byte array.
/// - Deserialized `Temp` becomes `Path` (temp ownership is not restored).
#[derive(Debug)]
pub enum FileSource {
    /// Local filesystem path.
    Path(PathBuf),
    /// Remote URL (will be streamed on read, not eagerly downloaded).
    Url(String),
    /// In-memory bytes (for small files or test fixtures).
    Bytes(Bytes),
    /// Managed temporary file (auto-deleted on drop).
    Temp(TempFile),
}

impl Clone for FileSource {
    fn clone(&self) -> Self {
        match self {
            Self::Path(path) => Self::Path(path.clone()),
            Self::Url(url) => Self::Url(url.clone()),
            Self::Bytes(bytes) => Self::Bytes(bytes.clone()),
            Self::Temp(temp) => match temp.try_clone() {
                Ok(cloned) => Self::Temp(cloned),
                Err(error) => {
                    tracing::warn!(
                        error = ?error,
                        path = %temp.path().display(),
                        "FileSource::Temp clone failed, falling back to Path"
                    );
                    Self::Path(temp.path().to_path_buf())
                }
            },
        }
    }
}

// -- Custom serde: Temp serializes as Path, Bytes as a byte vec --

mod serde_impl {
    use super::*;
    use serde::{Deserialize, Deserializer, Serialize, Serializer};

    #[derive(Serialize, Deserialize)]
    #[serde(tag = "type", content = "value")]
    enum FileSourceRepr {
        Path(PathBuf),
        Url(String),
        Bytes(Vec<u8>),
    }

    impl Serialize for FileSource {
        fn serialize<S: Serializer>(&self, ser: S) -> Result<S::Ok, S::Error> {
            match self {
                FileSource::Path(p) => FileSourceRepr::Path(p.clone()).serialize(ser),
                FileSource::Url(u) => FileSourceRepr::Url(u.clone()).serialize(ser),
                FileSource::Bytes(b) => FileSourceRepr::Bytes(b.to_vec()).serialize(ser),
                FileSource::Temp(t) => FileSourceRepr::Path(t.path().to_path_buf()).serialize(ser),
            }
        }
    }

    impl<'de> Deserialize<'de> for FileSource {
        fn deserialize<D: Deserializer<'de>>(de: D) -> Result<Self, D::Error> {
            let repr = FileSourceRepr::deserialize(de)?;
            Ok(match repr {
                FileSourceRepr::Path(p) => FileSource::Path(p),
                FileSourceRepr::Url(u) => FileSource::Url(u),
                FileSourceRepr::Bytes(b) => FileSource::Bytes(Bytes::from(b)),
            })
        }
    }
}

impl FileSource {
    /// Create a source from a local file path.
    pub fn from_path(p: impl Into<PathBuf>) -> Self {
        Self::Path(p.into())
    }

    /// Create a source from a remote URL.
    pub fn from_url(url: impl Into<String>) -> Self {
        Self::Url(url.into())
    }

    /// Create a source from in-memory bytes.
    pub fn from_bytes(b: impl Into<Bytes>) -> Self {
        Self::Bytes(b.into())
    }

    /// Open an async reader over this source.
    pub async fn reader(&self) -> AppResult<Box<dyn AsyncRead + Send + Unpin>> {
        match self {
            Self::Path(p) => {
                let file = file::open(p).await.map_err(|e| {
                    AppError::new(
                        ErrorCode::NotFound,
                        format!("failed to open {}: {e}", p.display()),
                    )
                })?;
                Ok(Box::new(file))
            }
            Self::Bytes(b) => Ok(Box::new(std::io::Cursor::new(b.clone()))),
            Self::Temp(t) => {
                let file = file::open(t.path()).await.map_err(|e| {
                    AppError::new(
                        ErrorCode::Internal,
                        format!("failed to open temp file {}: {e}", t.path().display()),
                    )
                })?;
                Ok(Box::new(file))
            }
            Self::Url(_url) => Err(AppError::new(
                ErrorCode::InvalidInput,
                "URL sources require an HTTP client; use to_local_path() first",
            )),
        }
    }

    /// Open a byte stream over this source.
    pub async fn stream(&self) -> AppResult<impl Stream<Item = AppResult<Bytes>> + '_> {
        use futures::stream;
        use tokio::io::AsyncReadExt;

        let mut reader = self.reader().await?;
        let (tx, rx) = tokio::sync::mpsc::channel::<AppResult<Bytes>>(8);

        let mut buf = vec![0u8; 64 * 1024];
        tokio::spawn(async move {
            loop {
                match reader.read(&mut buf).await {
                    Ok(0) => break,
                    Ok(n) => {
                        if tx
                            .send(Ok(Bytes::copy_from_slice(&buf[..n])))
                            .await
                            .is_err()
                        {
                            break;
                        }
                    }
                    Err(e) => {
                        let _ = tx
                            .send(Err(AppError::new(
                                ErrorCode::Internal,
                                format!("stream read error: {e}"),
                            )))
                            .await;
                        break;
                    }
                }
            }
        });

        Ok(stream::unfold(rx, |mut rx| async move {
            rx.recv().await.map(|item| (item, rx))
        }))
    }

    /// Read entire content into memory (use only for small files).
    pub async fn read_all(&self) -> AppResult<Bytes> {
        match self {
            Self::Bytes(b) => Ok(b.clone()),
            _ => {
                use tokio::io::AsyncReadExt;
                let mut reader = self.reader().await?;
                let mut buf = Vec::new();
                reader.read_to_end(&mut buf).await.map_err(|e| {
                    AppError::new(ErrorCode::Internal, format!("failed to read file: {e}"))
                })?;
                Ok(Bytes::from(buf))
            }
        }
    }

    /// Size in bytes (may require a stat call).
    pub async fn size(&self) -> AppResult<Option<u64>> {
        match self {
            Self::Path(p) => {
                let meta = file::metadata(p).await.map_err(|e| {
                    AppError::new(
                        ErrorCode::NotFound,
                        format!("failed to stat {}: {e}", p.display()),
                    )
                })?;
                Ok(Some(meta.len))
            }
            Self::Bytes(b) => Ok(Some(b.len() as u64)),
            Self::Temp(t) => {
                let meta = file::metadata(t.path()).await.map_err(|e| {
                    AppError::new(
                        ErrorCode::Internal,
                        format!("failed to stat temp file: {e}"),
                    )
                })?;
                Ok(Some(meta.len))
            }
            Self::Url(_) => Ok(None),
        }
    }

    /// Resolve to a local file path. Downloads to temp if source is URL/Bytes.
    pub async fn to_local_path(&self) -> AppResult<ResolvedPath> {
        match self {
            Self::Path(p) => Ok(ResolvedPath {
                path: p.clone(),
                _temp: None,
            }),
            Self::Temp(t) => Ok(ResolvedPath {
                path: t.path().to_path_buf(),
                _temp: None,
            }),
            Self::Bytes(b) => {
                let tmp = TempFile::new()?;
                file::write(tmp.path(), b).await.map_err(|e| {
                    AppError::new(
                        ErrorCode::Internal,
                        format!("failed to write bytes to temp file: {e}"),
                    )
                })?;
                let path = tmp.path().to_path_buf();
                Ok(ResolvedPath {
                    path,
                    _temp: Some(tmp),
                })
            }
            Self::Url(_url) => Err(AppError::new(
                ErrorCode::InvalidInput,
                "URL download not yet implemented; use an HTTP client externally",
            )),
        }
    }

    /// File extension (from path or URL), if detectable.
    pub fn extension(&self) -> Option<&str> {
        match self {
            Self::Path(p) => p.extension().and_then(|e| e.to_str()),
            Self::Url(url) => {
                let path = url.split('?').next().unwrap_or(url);
                path.rsplit('.').next().filter(|ext| ext.len() <= 10)
            }
            Self::Temp(t) => t.path().extension().and_then(|e| e.to_str()),
            Self::Bytes(_) => None,
        }
    }
}

/// A local path that may be backed by a temp file.
/// The temp file (if any) is kept alive as long as this struct exists.
pub struct ResolvedPath {
    path: PathBuf,
    _temp: Option<TempFile>,
}

impl ResolvedPath {
    /// The local file path.
    pub fn path(&self) -> &Path {
        &self.path
    }
}

impl AsRef<Path> for ResolvedPath {
    fn as_ref(&self) -> &Path {
        &self.path
    }
}

impl std::fmt::Debug for ResolvedPath {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ResolvedPath")
            .field("path", &self.path)
            .finish()
    }
}

#[cfg(test)]
mod tests {
    use futures::StreamExt;
    use serde_json::json;
    use tokio::io::AsyncReadExt;

    use super::*;

    #[tokio::test]
    async fn file_source_serializes_paths_urls_bytes_and_temp_as_stable_representations() {
        let path = FileSource::from_path("data/file.txt");
        assert_eq!(
            serde_json::to_value(&path).unwrap(),
            json!({"type":"Path","value":"data/file.txt"})
        );
        assert!(matches!(path.clone(), FileSource::Path(_)));

        let url = FileSource::from_url("https://example.invalid/file.bin");
        assert_eq!(
            serde_json::to_value(&url).unwrap(),
            json!({"type":"Url","value":"https://example.invalid/file.bin"})
        );
        assert!(matches!(url.clone(), FileSource::Url(_)));

        let bytes = FileSource::from_bytes(Bytes::from_static(b"abc"));
        assert_eq!(
            serde_json::to_value(&bytes).unwrap(),
            json!({"type":"Bytes","value":[97,98,99]})
        );
        assert!(matches!(bytes.clone(), FileSource::Bytes(_)));

        let temp = TempFile::new().unwrap();
        let value = serde_json::to_value(FileSource::Temp(temp)).unwrap();
        assert_eq!(value["type"], "Path");

        let decoded: FileSource =
            serde_json::from_value(json!({"type":"Bytes","value":[1,2,3]})).unwrap();
        assert_eq!(
            decoded.read_all().await.unwrap(),
            Bytes::from_static(&[1, 2, 3])
        );
    }

    #[tokio::test]
    async fn file_source_reads_sizes_streams_and_resolves_local_paths() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("input.txt");
        tokio::fs::write(&path, b"path-data").await.unwrap();

        let path_source = FileSource::from_path(&path);
        assert_eq!(path_source.extension(), Some("txt"));
        assert_eq!(path_source.size().await.unwrap(), Some(9));
        assert_eq!(
            path_source.read_all().await.unwrap(),
            Bytes::from_static(b"path-data")
        );
        assert_eq!(
            path_source.to_local_path().await.unwrap().path(),
            path.as_path()
        );

        let mut reader = FileSource::from_bytes(Bytes::from_static(b"reader"))
            .reader()
            .await
            .unwrap();
        let mut data = Vec::new();
        reader.read_to_end(&mut data).await.unwrap();
        assert_eq!(data, b"reader");

        let bytes_source = FileSource::from_bytes(Bytes::from_static(b"stream-data"));
        let streamed = {
            let stream = bytes_source.stream().await.unwrap();
            futures::pin_mut!(stream);
            let mut streamed = Vec::new();
            while let Some(chunk) = stream.next().await {
                streamed.extend_from_slice(&chunk.unwrap());
            }
            streamed
        };
        assert_eq!(streamed, b"stream-data");

        let resolved = bytes_source.to_local_path().await.unwrap();
        assert_eq!(
            tokio::fs::read(resolved.path()).await.unwrap(),
            b"stream-data"
        );
        assert!(format!("{resolved:?}").contains("ResolvedPath"));
        assert_eq!(resolved.as_ref(), resolved.path());
    }

    #[tokio::test]
    async fn file_source_handles_temp_url_and_missing_file_branches() {
        let temp = TempFile::new().unwrap();
        tokio::fs::write(temp.path(), b"temporary").await.unwrap();
        let source = FileSource::Temp(temp);
        assert_eq!(source.size().await.unwrap(), Some(9));
        assert_eq!(
            source.read_all().await.unwrap(),
            Bytes::from_static(b"temporary")
        );
        assert_eq!(
            source.to_local_path().await.unwrap().path().extension(),
            None
        );
        assert!(matches!(source.clone(), FileSource::Temp(_)));

        let url = FileSource::from_url("https://example.invalid/file.tar.gz?token=redacted");
        assert_eq!(url.extension(), Some("gz"));
        assert_eq!(url.size().await.unwrap(), None);
        assert_eq!(
            url.reader().await.err().unwrap().code(),
            ErrorCode::InvalidInput
        );
        assert_eq!(
            url.to_local_path().await.unwrap_err().code(),
            ErrorCode::InvalidInput
        );

        let missing = FileSource::from_path("missing-file.bin");
        assert_eq!(
            missing.reader().await.err().unwrap().code(),
            ErrorCode::NotFound
        );
        assert_eq!(
            missing.size().await.unwrap_err().code(),
            ErrorCode::NotFound
        );
    }

    #[tokio::test]
    async fn url_and_bytes_metadata_cover_fallback_mime_paths() {
        let url = FileSource::from_url("https://example.invalid/archive.tar.gz?token=redacted");
        let meta = crate::file_meta(&url).await.unwrap();
        assert_eq!(meta.name.as_deref(), Some("archive.tar.gz"));
        assert_eq!(meta.extension.as_deref(), Some("gz"));
        assert_eq!(meta.size, None);
        assert_eq!(
            crate::detect_kind(&url).await.unwrap(),
            crate::FileKind::Archive
        );

        let bytes = FileSource::from_bytes(Bytes::from_static(b"plain text"));
        let meta = crate::file_meta(&bytes).await.unwrap();
        assert_eq!(meta.name, None);
        assert_eq!(meta.size, Some(10));
        assert_eq!(meta.mime_type, "application/octet-stream");
        assert_eq!(bytes.extension(), None);
    }

    #[tokio::test]
    async fn path_metadata_uses_filesystem_timestamps_and_extension_mime() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("image.png");
        tokio::fs::write(&path, b"not a real png").await.unwrap();
        let source = FileSource::from_path(&path);

        let meta = crate::file_meta(&source).await.unwrap();

        assert_eq!(meta.name.as_deref(), Some("image.png"));
        assert_eq!(meta.extension.as_deref(), Some("png"));
        assert_eq!(meta.mime_type, "image/png");
        assert_eq!(meta.size, Some(14));
        assert!(meta.modified_at.is_some());
    }

    #[test]
    fn extension_filters_overlong_url_suffixes() {
        let url = FileSource::from_url("https://example.invalid/file.thisextensionistoolong");

        assert_eq!(url.extension(), None);
    }
}
