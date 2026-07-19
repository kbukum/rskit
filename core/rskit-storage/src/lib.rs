//! File I/O, storage backends, temp files, and MIME detection.
//!
//! `rskit-storage` provides generic file operations for any file type: read, write, copy, stream,
//! detect type, manage temp files, and store through the [`FileStore`] trait.

#![warn(missing_docs)]

mod meta;
mod sink;
mod source;
/// Storage backends for file persistence.
pub mod store;
mod temp;
mod transfer;

pub use meta::{FileKind, FileMeta, detect_kind, detect_mime, file_meta};
pub use sink::{FileSink, FileWriter};
pub use source::{FileSource, ResolvedPath};
pub use store::{
    DEFAULT_CONTENT_TYPE, FileStore, LocalStore, LocalStoreConfig, ProgressCallback, StorageConfig,
    StorageFactory, StorageRegistry, StoredFile, UploadProgress, content_type_or_default,
    prefixed_key, register_local,
};
pub use temp::{TempDir, TempFile};
pub use transfer::{copy_file, transfer};
