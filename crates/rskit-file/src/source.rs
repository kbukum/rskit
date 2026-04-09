//! File source — a reference to file content that can be read.

use std::path::{Path, PathBuf};

use bytes::Bytes;
use futures::Stream;
use rskit_errors::{AppError, AppResult, ErrorCode};
use tokio::io::AsyncRead;

use crate::TempFile;

/// A reference to file content that can be read.
/// Does NOT load content eagerly — all reads are lazy/streamed.
///
/// Serialization notes:
/// - `Temp` serializes as `Path` (temp file's path).
/// - `Bytes` serializes as a byte array.
/// - Deserialized `Temp` becomes `Path` (temp ownership is not restored).
#[derive(Debug, Clone)]
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
                let file = tokio::fs::File::open(p).await.map_err(|e| {
                    AppError::new(
                        ErrorCode::NotFound,
                        format!("failed to open {}: {e}", p.display()),
                    )
                })?;
                Ok(Box::new(file))
            }
            Self::Bytes(b) => Ok(Box::new(std::io::Cursor::new(b.clone()))),
            Self::Temp(t) => {
                let file = tokio::fs::File::open(t.path()).await.map_err(|e| {
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
                let meta = tokio::fs::metadata(p).await.map_err(|e| {
                    AppError::new(
                        ErrorCode::NotFound,
                        format!("failed to stat {}: {e}", p.display()),
                    )
                })?;
                Ok(Some(meta.len()))
            }
            Self::Bytes(b) => Ok(Some(b.len() as u64)),
            Self::Temp(t) => {
                let meta = tokio::fs::metadata(t.path()).await.map_err(|e| {
                    AppError::new(
                        ErrorCode::Internal,
                        format!("failed to stat temp file: {e}"),
                    )
                })?;
                Ok(Some(meta.len()))
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
                tokio::fs::write(tmp.path(), b).await.map_err(|e| {
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
