//! File I/O, storage backends, temp files, and MIME detection.
//!
//! `rskit-file` provides generic file operations for any file type:
//! read, write, copy, stream, detect type, manage temp files, and
//! store to local/cloud backends.

#![warn(missing_docs)]

mod source;
mod sink;
mod meta;
mod temp;
mod transfer;
/// Storage backends for file persistence.
pub mod store;

pub use source::{FileSource, ResolvedPath};
pub use sink::{FileSink, FileWriter};
pub use meta::{FileMeta, FileKind, detect_mime, detect_kind, file_meta};
pub use temp::{TempFile, TempDir};
pub use transfer::{copy_file, transfer};
pub use store::{
    FileStore, StoredFile, UploadProgress, ProgressCallback,
    LocalStore, LocalStoreConfig,
};

#[cfg(feature = "s3")]
pub use store::{S3Store, S3StoreConfig};

#[cfg(feature = "gcs")]
pub use store::{GcsStore, GcsStoreConfig};
