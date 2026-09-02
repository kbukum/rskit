//! File metadata and MIME type detection.

use chrono::{DateTime, Utc};
use rskit_errors::AppResult;
use rskit_fs::async_io::file;
use serde::{Deserialize, Serialize};

use crate::FileSource;

/// Metadata about a file.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FileMeta {
    /// Original file name, if known.
    pub name: Option<String>,
    /// File extension, if known.
    pub extension: Option<String>,
    /// MIME type (e.g., "video/mp4").
    pub mime_type: String,
    /// File size in bytes.
    pub size: Option<u64>,
    /// Creation timestamp.
    pub created_at: Option<DateTime<Utc>>,
    /// Last modification timestamp.
    pub modified_at: Option<DateTime<Utc>>,
    /// Checksum (e.g., SHA-256 hex string).
    pub checksum: Option<String>,
}

/// Broad file category for routing to the right processor.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum FileKind {
    /// Video file (e.g., mp4, mkv, webm).
    Video,
    /// Audio file (e.g., mp3, wav, flac).
    Audio,
    /// Image file (e.g., png, jpeg, webp).
    Image,
    /// Document (e.g., pdf, docx).
    Document,
    /// Archive (e.g., zip, tar.gz).
    Archive,
    /// Plain text file.
    Text,
    /// Generic binary file.
    Binary,
    /// Could not determine file type.
    Unknown,
}

impl FileKind {
    /// Determine the file kind from a MIME type string.
    pub fn from_mime(mime: &str) -> Self {
        let main = mime.split('/').next().unwrap_or("");
        match main {
            "video" => Self::Video,
            "audio" => Self::Audio,
            "image" => Self::Image,
            "text" => Self::Text,
            "application" => {
                if mime.contains("pdf")
                    || mime.contains("document")
                    || mime.contains("msword")
                    || mime.contains("spreadsheet")
                    || mime.contains("presentation")
                {
                    Self::Document
                } else if mime.contains("zip")
                    || mime.contains("tar")
                    || mime.contains("gzip")
                    || mime.contains("bzip")
                    || mime.contains("rar")
                    || mime.contains("7z")
                    || mime.contains("xz")
                {
                    Self::Archive
                } else if mime.contains("json")
                    || mime.contains("xml")
                    || mime.contains("yaml")
                    || mime.contains("javascript")
                {
                    Self::Text
                } else {
                    Self::Binary
                }
            }
            _ => Self::Unknown,
        }
    }
}

/// Detect the MIME type of a file source.
///
/// Uses magic-byte detection on the first bytes of the file, falling back to extension-based guessing.
pub async fn detect_mime(source: &FileSource) -> AppResult<String> {
    // Try magic-byte detection first
    let bytes = match source {
        FileSource::Bytes(b) => Some(b.clone()),
        FileSource::Path(_) | FileSource::Temp(_) => source.read_all().await.ok(),
        FileSource::Url(_) => None,
    };

    if let Some(bytes) = &bytes
        && let Some(kind) = infer::get(bytes)
    {
        return Ok(kind.mime_type().to_string());
    }

    // Fall back to extension-based guessing
    if let Some(ext) = source.extension() {
        let guess = mime_guess::from_ext(ext).first_or_octet_stream();
        return Ok(guess.to_string());
    }

    Ok("application/octet-stream".to_string())
}

/// Detect the broad file kind of a source.
pub async fn detect_kind(source: &FileSource) -> AppResult<FileKind> {
    let mime = detect_mime(source).await?;
    Ok(FileKind::from_mime(&mime))
}

/// Extract full metadata from a file source.
pub async fn file_meta(source: &FileSource) -> AppResult<FileMeta> {
    let mime_type = detect_mime(source).await?;
    let size = source.size().await?;

    let (name, extension, created_at, modified_at) = match source {
        FileSource::Path(p) => {
            let name = p.file_name().map(|n| n.to_string_lossy().to_string());
            let ext = p.extension().map(|e| e.to_string_lossy().to_string());
            let (created, modified) = match file::metadata(p).await {
                Ok(meta) => {
                    let created = meta.created.map(DateTime::<Utc>::from);
                    let modified = meta.modified.map(DateTime::<Utc>::from);
                    (created, modified)
                }
                Err(_) => (None, None),
            };
            (name, ext, created, modified)
        }
        FileSource::Url(url) => {
            let path_part = url.split('?').next().unwrap_or(url);
            let name = path_part.rsplit('/').next().map(String::from);
            let ext = source.extension().map(String::from);
            (name, ext, None, None)
        }
        FileSource::Temp(t) => {
            let name = t
                .path()
                .file_name()
                .map(|n| n.to_string_lossy().to_string());
            let ext = t
                .path()
                .extension()
                .map(|e| e.to_string_lossy().to_string());
            (name, ext, None, None)
        }
        FileSource::Bytes(_) => (None, None, None, None),
    };

    Ok(FileMeta {
        name,
        extension,
        mime_type,
        size,
        created_at,
        modified_at,
        checksum: None,
    })
}

#[cfg(test)]
mod tests {
    use bytes::Bytes;

    use super::*;
    use crate::TempFile;

    #[test]
    fn file_meta_serde_pins_checksum_and_wire_field_names() {
        let meta = FileMeta {
            name: Some("clip.mp4".to_string()),
            extension: Some("mp4".to_string()),
            mime_type: "video/mp4".to_string(),
            size: Some(2048),
            created_at: None,
            modified_at: None,
            checksum: Some("abc123".to_string()),
        };

        let value = serde_json::to_value(&meta).unwrap();

        assert_eq!(value["name"], "clip.mp4");
        assert_eq!(value["extension"], "mp4");
        assert_eq!(value["mime_type"], "video/mp4");
        assert_eq!(value["size"], 2048);
        assert_eq!(value["checksum"], "abc123");

        let round_trip: FileMeta = serde_json::from_value(value).unwrap();
        assert_eq!(round_trip.checksum.as_deref(), Some("abc123"));
        assert_eq!(round_trip.mime_type, "video/mp4");
    }

    #[test]
    fn file_kind_classifies_common_mime_families() {
        assert_eq!(FileKind::from_mime("video/mp4"), FileKind::Video);
        assert_eq!(FileKind::from_mime("audio/mpeg"), FileKind::Audio);
        assert_eq!(FileKind::from_mime("image/png"), FileKind::Image);
        assert_eq!(FileKind::from_mime("text/plain"), FileKind::Text);
        assert_eq!(FileKind::from_mime("application/pdf"), FileKind::Document);
        assert_eq!(
            FileKind::from_mime(
                "application/vnd.openxmlformats-officedocument.spreadsheetml.sheet"
            ),
            FileKind::Document
        );
        assert_eq!(FileKind::from_mime("application/zip"), FileKind::Archive);
        assert_eq!(FileKind::from_mime("application/json"), FileKind::Text);
        assert_eq!(
            FileKind::from_mime("application/octet-stream"),
            FileKind::Binary
        );
        assert_eq!(FileKind::from_mime("custom"), FileKind::Unknown);
    }

    #[tokio::test]
    async fn mime_detection_prefers_magic_then_extension_then_default() {
        let png = FileSource::from_bytes(Bytes::from_static(&[
            0x89, b'P', b'N', b'G', b'\r', b'\n', 0x1a, b'\n', 0, 0, 0, 0,
        ]));
        assert_eq!(detect_mime(&png).await.unwrap(), "image/png");
        assert_eq!(detect_kind(&png).await.unwrap(), FileKind::Image);

        let by_extension = FileSource::from_path("report.pdf");
        assert_eq!(detect_mime(&by_extension).await.unwrap(), "application/pdf");

        let url = FileSource::from_url("https://example.invalid/download");
        assert_eq!(detect_mime(&url).await.unwrap(), "application/octet-stream");
    }

    #[tokio::test]
    async fn file_meta_extracts_path_url_temp_and_bytes_details() {
        let dir = tempfile::tempdir().unwrap();
        let file = dir.path().join("sample.txt");
        tokio::fs::write(&file, b"hello").await.unwrap();

        let path_meta = file_meta(&FileSource::from_path(&file)).await.unwrap();
        assert_eq!(path_meta.name.as_deref(), Some("sample.txt"));
        assert_eq!(path_meta.extension.as_deref(), Some("txt"));
        assert_eq!(path_meta.mime_type, "text/plain");
        assert_eq!(path_meta.size, Some(5));
        assert!(path_meta.modified_at.is_some());

        let url_meta = file_meta(&FileSource::from_url(
            "https://example.invalid/assets/archive.tar.gz?download=1",
        ))
        .await
        .unwrap();
        assert_eq!(url_meta.name.as_deref(), Some("archive.tar.gz"));
        assert_eq!(url_meta.extension.as_deref(), Some("gz"));
        assert_eq!(url_meta.size, None);

        let temp = TempFile::new().unwrap();
        tokio::fs::write(temp.path(), b"temp").await.unwrap();
        let temp_meta = file_meta(&FileSource::Temp(temp)).await.unwrap();
        assert_eq!(temp_meta.size, Some(4));

        let bytes_meta = file_meta(&FileSource::from_bytes(Bytes::from_static(b"bytes")))
            .await
            .unwrap();
        assert_eq!(bytes_meta.name, None);
        assert_eq!(bytes_meta.size, Some(5));
    }
}
