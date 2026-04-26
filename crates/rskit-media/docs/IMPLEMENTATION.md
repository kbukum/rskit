# rskit — Media & File Modules Implementation Roadmap

Complete specification for file I/O, storage, and media processing capabilities.
Each section is self-contained: types, public API, file location, dependencies, and implementation notes.

**Design principles:**
- **Open/Closed** — add new codecs, formats, and filters without modifying core types
- **Data-driven** — compatibility and metadata live in a registry, not hardcoded match arms
- **Pipeline-first** — lazy chainable builder compiled to optimized backend commands
- **Stream-aware** — large files never fully loaded; all I/O is streamed
- **One concern per file** — shallow module hierarchy, easy to navigate

---

## Table of Contents

1. [Crate Overview](#1-crate-overview)
2. [rskit-file — File I/O & Storage](#2-rskit-file--file-io--storage)
3. [rskit-media — Core Types, Traits & Pipeline](#3-rskit-media--core-types-traits--pipeline)
4. [rskit-media-ffmpeg — FFmpeg Backend](#4-rskit-media-ffmpeg--ffmpeg-backend)
5. [rskit-media-image — Image Backend](#5-rskit-media-image--image-backend)
6. [Workspace Changes](#6-workspace-changes)
7. [Dependency Reference](#7-dependency-reference)
8. [Implementation Order](#8-implementation-order)

---

## 1. Crate Overview

| Crate | Purpose | Heavy deps |
|---|---|---|
| `rskit-file` | File I/O, storage backends, temp files, MIME detection | `tokio-fs`, `aws-sdk-s3` (opt), `google-cloud-storage` (opt) |
| `rskit-media` | Media types, codec/format registry, pipeline builder, traits | None (pure types + trait defs) |
| `rskit-media-ffmpeg` | FFmpeg CLI backend for video/audio processing | `tokio::process` |
| `rskit-media-image` | Native image processing (no FFmpeg needed for images) | `image` crate |

**Separation of concerns:**

```
rskit-file          Pure file I/O — any file type, any storage backend
    ↑
rskit-media         Media types + pipeline builder + traits (no processing logic)
    ↑               ↑
rskit-media-ffmpeg  Video/audio backend     rskit-media-image  Image backend
```

`rskit-file` knows nothing about media. `rskit-media` knows nothing about FFmpeg.
Backends are swappable. The pipeline builder is backend-agnostic.

---

## 2. `rskit-file` — File I/O & Storage

**What it does:** Generic file operations for any file type. Read, write, copy,
move, delete, stream, detect type, manage temp files, and store to local/cloud
backends. This is the I/O foundation that all other crates build on.

**Location:** `crates/rskit-file/`

**Key dependencies:**

```toml
[dependencies]
rskit-errors    = { path = "../rskit-errors" }
rskit-config    = { path = "../rskit-config" }
rskit-bootstrap = { path = "../rskit-bootstrap" }
tokio           = { workspace = true, features = ["fs", "io-util"] }
bytes           = { workspace = true }
serde           = { workspace = true }
tracing         = { workspace = true }
tempfile        = "3"
mime_guess      = "2"
infer           = "0.19"
futures-core    = { workspace = true }
pin-project-lite = "0.2"

[features]
default = []
s3  = ["dep:aws-sdk-s3", "dep:aws-config"]
gcs = ["dep:google-cloud-storage"]

[dependencies.aws-sdk-s3]
version  = "1"
optional = true

[dependencies.aws-config]
version  = "1"
optional = true

[dependencies.google-cloud-storage]
version  = "0.22"
optional = true
```

### Module Structure

```
src/
├── lib.rs              # mod declarations + pub use re-exports
├── source.rs           # FileSource, ResolvedPath
├── sink.rs             # FileSink, FileWriter
├── meta.rs             # FileMeta, FileKind, detect_mime, detect_kind, file_meta
├── temp.rs             # TempFile, TempDir
├── transfer.rs         # copy_file, transfer (streaming I/O helpers)
├── store/
│   ├── mod.rs          # FileStore trait, StoredFile, UploadProgress, ProgressCallback
│   ├── local.rs        # LocalStore, LocalStoreConfig
│   ├── s3.rs           # S3Store, S3StoreConfig (feature: s3)
│   └── gcs.rs          # GcsStore, GcsStoreConfig (feature: gcs)
```

**`src/lib.rs`:**

```rust
mod source;
mod sink;
mod meta;
mod temp;
mod transfer;
mod store;

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
```

---

### 2.1 Source & Sink Types

**File:** `src/source.rs`

```rust
/// A reference to file content that can be read.
/// Does NOT load content eagerly — all reads are lazy/streamed.
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

impl FileSource {
    pub fn from_path(p: impl Into<PathBuf>) -> Self;
    pub fn from_url(url: impl Into<String>) -> Self;
    pub fn from_bytes(b: impl Into<Bytes>) -> Self;

    /// Open an async reader over this source.
    pub async fn reader(&self) -> AppResult<Box<dyn AsyncRead + Send + Unpin>>;

    /// Open a byte stream over this source.
    pub async fn stream(&self) -> AppResult<impl Stream<Item = AppResult<Bytes>>>;

    /// Read entire content into memory (use only for small files).
    pub async fn read_all(&self) -> AppResult<Bytes>;

    /// Size in bytes (may require a HEAD request for URLs).
    pub async fn size(&self) -> AppResult<Option<u64>>;

    /// Resolve to a local file path. Downloads to temp if source is URL/Bytes.
    pub async fn to_local_path(&self) -> AppResult<ResolvedPath>;

    /// File extension (from path or URL), if detectable.
    pub fn extension(&self) -> Option<&str>;
}

/// A local path that may be backed by a temp file.
pub struct ResolvedPath {
    path: PathBuf,
    _temp: Option<TempFile>,
}

impl ResolvedPath {
    pub fn path(&self) -> &Path;
}

impl AsRef<Path> for ResolvedPath { ... }
```

**File:** `src/sink.rs`

```rust
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
    pub async fn writer(&self) -> AppResult<FileWriter>;
}

/// Handle for writing output. Finalize to get the resulting FileSource.
pub struct FileWriter { ... }

impl FileWriter {
    pub async fn write_all(&mut self, data: &[u8]) -> AppResult<()>;
    pub async fn write_stream(&mut self, stream: impl Stream<Item = AppResult<Bytes>>) -> AppResult<()>;
    pub async fn finalize(self) -> AppResult<FileSource>;
}
```

---

### 2.2 File Metadata & MIME Detection

**File:** `src/meta.rs`

```rust
/// Metadata about a file.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FileMeta {
    pub name: Option<String>,
    pub extension: Option<String>,
    pub mime_type: String,
    pub size: Option<u64>,
    pub created_at: Option<DateTime<Utc>>,
    pub modified_at: Option<DateTime<Utc>>,
    pub checksum: Option<String>,
}

/// Broad file category for routing to the right processor.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum FileKind {
    Video,
    Audio,
    Image,
    Document,
    Archive,
    Text,
    Binary,
    Unknown,
}

impl FileKind {
    pub fn from_mime(mime: &str) -> Self;
}

pub async fn detect_mime(source: &FileSource) -> AppResult<String>;
pub async fn detect_kind(source: &FileSource) -> AppResult<FileKind>;
pub async fn file_meta(source: &FileSource) -> AppResult<FileMeta>;
```

---

### 2.3 FileStore Trait & Backends

**File:** `src/store/mod.rs`

```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StoredFile {
    pub key: String,
    pub size: u64,
    pub content_type: String,
    pub stored_at: DateTime<Utc>,
    pub metadata: HashMap<String, String>,
}

pub struct UploadProgress {
    pub bytes_sent: u64,
    pub total_bytes: Option<u64>,
    pub percent: Option<f32>,
}

pub type ProgressCallback = Arc<dyn Fn(UploadProgress) + Send + Sync>;

#[async_trait]
pub trait FileStore: Send + Sync {
    async fn upload(
        &self,
        source: &FileSource,
        key: &str,
        content_type: Option<&str>,
        metadata: Option<HashMap<String, String>>,
    ) -> AppResult<StoredFile>;

    async fn upload_with_progress(
        &self,
        source: &FileSource,
        key: &str,
        content_type: Option<&str>,
        on_progress: ProgressCallback,
    ) -> AppResult<StoredFile>;

    async fn download(&self, key: &str) -> AppResult<FileSource>;
    async fn download_stream(&self, key: &str) -> AppResult<impl Stream<Item = AppResult<Bytes>>>;
    async fn delete(&self, key: &str) -> AppResult<()>;
    async fn exists(&self, key: &str) -> AppResult<bool>;
    async fn head(&self, key: &str) -> AppResult<StoredFile>;
    async fn list(&self, prefix: &str, limit: Option<usize>) -> AppResult<Vec<StoredFile>>;
    async fn presigned_url(&self, key: &str, expires_in: Duration) -> AppResult<String>;
    async fn copy(&self, from_key: &str, to_key: &str) -> AppResult<StoredFile>;
    async fn rename(&self, from_key: &str, to_key: &str) -> AppResult<StoredFile>;
}
```

**File:** `src/store/local.rs`

```rust
#[derive(Debug, Clone, Deserialize)]
pub struct LocalStoreConfig {
    pub root_dir: PathBuf,
    pub auto_create: bool,
}

pub struct LocalStore { config: LocalStoreConfig }

impl LocalStore {
    pub fn new(config: LocalStoreConfig) -> AppResult<Self>;
}

#[async_trait]
impl FileStore for LocalStore { ... }
```

**File:** `src/store/s3.rs`

```rust
#[cfg(feature = "s3")]
#[derive(Debug, Clone, Deserialize)]
pub struct S3StoreConfig {
    pub bucket: String,
    pub region: Option<String>,
    pub endpoint: Option<String>,
    pub prefix: Option<String>,
}

#[cfg(feature = "s3")]
pub struct S3Store { client: aws_sdk_s3::Client, config: S3StoreConfig }

#[cfg(feature = "s3")]
impl S3Store {
    pub async fn new(config: S3StoreConfig) -> AppResult<Self>;
}

#[cfg(feature = "s3")]
#[async_trait]
impl FileStore for S3Store { ... }
```

**File:** `src/store/gcs.rs`

```rust
#[cfg(feature = "gcs")]
#[derive(Debug, Clone, Deserialize)]
pub struct GcsStoreConfig {
    pub bucket: String,
    pub prefix: Option<String>,
}

#[cfg(feature = "gcs")]
pub struct GcsStore { ... }

#[cfg(feature = "gcs")]
#[async_trait]
impl FileStore for GcsStore { ... }
```

---

### 2.4 Temp File Management

**File:** `src/temp.rs`

```rust
/// Managed temporary file. Deleted when dropped.
#[derive(Debug)]
pub struct TempFile {
    inner: tempfile::NamedTempFile,
}

impl TempFile {
    pub fn new() -> AppResult<Self>;
    pub fn with_extension(ext: &str) -> AppResult<Self>;
    pub fn in_dir(dir: &Path) -> AppResult<Self>;
    pub fn in_dir_with_extension(dir: &Path, ext: &str) -> AppResult<Self>;
    pub fn path(&self) -> &Path;
    pub fn into_source(self) -> FileSource;
    pub fn persist(self, target: impl AsRef<Path>) -> AppResult<PathBuf>;
}

impl Clone for TempFile { ... }

/// Temp directory manager. All temp files created within are cleaned up on drop.
pub struct TempDir {
    inner: tempfile::TempDir,
}

impl TempDir {
    pub fn new() -> AppResult<Self>;
    pub fn path(&self) -> &Path;
    pub fn create_file(&self, name: &str) -> AppResult<TempFile>;
    pub fn create_file_with_extension(&self, ext: &str) -> AppResult<TempFile>;
}
```

---

### 2.5 Streaming I/O

**File:** `src/transfer.rs`

```rust
pub async fn copy_file(
    source: &FileSource,
    sink: &FileSink,
    on_progress: Option<ProgressCallback>,
) -> AppResult<FileSource>;

pub async fn transfer(
    from_store: &dyn FileStore,
    from_key: &str,
    to_store: &dyn FileStore,
    to_key: &str,
) -> AppResult<StoredFile>;
```

---

## 3. `rskit-media` — Core Types, Traits & Pipeline

**What it does:** Defines all media types, the codec/format registry, processing
traits, and the lazy chainable pipeline builder. This crate has NO processing
logic — it defines the vocabulary that backends implement.

**Location:** `crates/rskit-media/`

**Key dependencies:**

```toml
[dependencies]
rskit-errors = { path = "../rskit-errors" }
rskit-file   = { path = "../rskit-file" }
serde        = { workspace = true }
serde_json   = { workspace = true }
chrono       = { workspace = true }
tracing      = { workspace = true }
async-trait  = { workspace = true }
```

### Module Structure

```
src/
├── lib.rs              # mod declarations + pub use re-exports
├── types.rs            # MediaType, TrackKind (small finite enums)
├── time.rs             # Timestamp, TimeRange, Segment
├── spatial.rs          # Resolution, FrameRate
├── audio.rs            # SampleRate, ChannelLayout
├── track.rs            # Track, VideoTrackInfo, AudioTrackInfo, SubtitleTrackInfo
├── codec.rs            # Codec type + CodecKind + well-known constants
├── format.rs           # Format type + well-known constants
├── registry.rs         # Registry, CodecInfo, FormatInfo, compatibility
├── filter.rs           # Filter, FilterTarget, Params + convenience constructors
├── output.rs           # OutputConfig, VideoSettings, AudioSettings, Quality, Bitrate, EncodingSpeed
├── presets.rs          # Preset OutputConfig constructors
├── probe.rs            # MediaMetadata, MediaProbe trait
├── ops/
│   ├── mod.rs          # MediaOp enum (lean — references types from sibling files)
│   ├── spatial.rs      # ResizeOp, ResizeMode, CropRegion, Rotation, FlipDirection, PadOp
│   └── compose.rs      # OverlayOp, OverlayPosition, ConcatOp, Transition, ReplaceAudioOp, MixAudioOp
├── pipeline.rs         # MediaPipeline, Progress
├── executor.rs         # MediaExecutor trait
├── subtitle.rs         # SubtitleEntry, SubtitleStyle, SubtitlePosition, SubtitleTrack
```

**`src/lib.rs`:**

```rust
mod types;
mod time;
mod spatial;
mod audio;
mod track;
pub mod codec;
pub mod format;
mod registry;
pub mod filter;
mod output;
pub mod presets;
mod probe;
pub mod ops;
mod pipeline;
mod executor;
mod subtitle;

pub use types::{MediaType, TrackKind};
pub use time::{Timestamp, TimeRange, Segment};
pub use spatial::{Resolution, FrameRate};
pub use audio::{SampleRate, ChannelLayout};
pub use track::{Track, VideoTrackInfo, AudioTrackInfo, SubtitleTrackInfo};
pub use codec::{Codec, CodecKind};
pub use format::Format;
pub use registry::{Registry, CodecInfo, FormatInfo};
pub use filter::{Filter, FilterTarget, Params, ParamValue};
pub use output::{OutputConfig, VideoSettings, AudioSettings, Quality, Bitrate, EncodingSpeed};
pub use probe::{MediaMetadata, MediaProbe};
pub use ops::MediaOp;
pub use pipeline::{MediaPipeline, Progress};
pub use executor::MediaExecutor;
pub use subtitle::{SubtitleEntry, SubtitleStyle, SubtitlePosition, SubtitleTrack};
```

---

### 3.1 Core Types

**File:** `src/types.rs`

These are genuinely finite categories — closed enums are correct here.

```rust
/// The broad media category.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum MediaType {
    Video,
    Audio,
    Image,
}

/// Kind of track in a media container.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum TrackKind {
    Video,
    Audio,
    Subtitle,
    Data,
    Attachment,
}
```

---

### 3.2 Time Types

**File:** `src/time.rs`

```rust
/// A time point in milliseconds from the start of the media.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub struct Timestamp(pub u64);

impl Timestamp {
    pub fn from_millis(ms: u64) -> Self;
    pub fn from_seconds(s: f64) -> Self;
    pub fn from_hms(h: u32, m: u32, s: f64) -> Self;
    pub fn as_millis(&self) -> u64;
    pub fn as_seconds(&self) -> f64;
    pub fn as_duration(&self) -> Duration;
    /// Format as "HH:MM:SS.mmm" (FFmpeg-compatible).
    pub fn to_ffmpeg_time(&self) -> String;
}

impl std::fmt::Display for Timestamp { ... } // "01:23:45.678"

/// A time range within a media file.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct TimeRange {
    pub start: Timestamp,
    pub end: Timestamp,
}

impl TimeRange {
    pub fn new(start: Timestamp, end: Timestamp) -> Self;
    pub fn from_millis(start_ms: u64, end_ms: u64) -> Self;
    pub fn from_seconds(start: f64, end: f64) -> Self;
    pub fn duration(&self) -> Duration;
    pub fn duration_ms(&self) -> u64;
    pub fn contains(&self, ts: Timestamp) -> bool;
    pub fn overlaps(&self, other: &TimeRange) -> bool;
    pub fn merge(&self, other: &TimeRange) -> Option<TimeRange>;
    pub fn split_at(&self, ts: Timestamp) -> (Option<TimeRange>, Option<TimeRange>);
    pub fn shift(&self, offset: i64) -> Self;
}

/// A labeled segment within a media file.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Segment {
    pub range: TimeRange,
    pub label: Option<String>,
    pub confidence: Option<f32>,
    pub metadata: HashMap<String, serde_json::Value>,
}

impl Segment {
    pub fn new(range: TimeRange) -> Self;
    pub fn with_label(self, label: impl Into<String>) -> Self;
    pub fn with_confidence(self, c: f32) -> Self;
    pub fn with_meta(self, key: impl Into<String>, val: impl Into<serde_json::Value>) -> Self;
}
```

---

### 3.3 Spatial Types

**File:** `src/spatial.rs`

```rust
/// Width × Height in pixels.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct Resolution {
    pub width: u32,
    pub height: u32,
}

impl Resolution {
    pub fn new(w: u32, h: u32) -> Self;

    pub fn p360()  -> Self { Self::new(640, 360) }
    pub fn p480()  -> Self { Self::new(854, 480) }
    pub fn p720()  -> Self { Self::new(1280, 720) }
    pub fn p1080() -> Self { Self::new(1920, 1080) }
    pub fn p1440() -> Self { Self::new(2560, 1440) }
    pub fn p4k()   -> Self { Self::new(3840, 2160) }

    pub fn aspect_ratio(&self) -> (u32, u32);
    pub fn aspect_ratio_f64(&self) -> f64;
    pub fn is_portrait(&self) -> bool;
    pub fn is_landscape(&self) -> bool;
    pub fn is_square(&self) -> bool;
    pub fn pixel_count(&self) -> u64;
    pub fn scale_to_fit(&self, max_width: u32, max_height: u32) -> Self;
    pub fn scale_to_fill(&self, width: u32, height: u32) -> Self;
    pub fn scale_by(&self, factor: f64) -> Self;
}

/// Rational frame rate (numerator / denominator) for exact representation.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct FrameRate {
    pub num: u32,
    pub den: u32,
}

impl FrameRate {
    pub fn new(num: u32, den: u32) -> Self;
    pub fn fps(n: u32) -> Self { Self::new(n, 1) }

    pub fn fps_24()  -> Self { Self::new(24, 1) }
    pub fn fps_25()  -> Self { Self::new(25, 1) }
    pub fn fps_30()  -> Self { Self::new(30, 1) }
    pub fn fps_50()  -> Self { Self::new(50, 1) }
    pub fn fps_60()  -> Self { Self::new(60, 1) }
    pub fn ntsc_30() -> Self { Self::new(30000, 1001) }
    pub fn ntsc_24() -> Self { Self::new(24000, 1001) }
    pub fn ntsc_60() -> Self { Self::new(60000, 1001) }

    pub fn as_f64(&self) -> f64;
}
```

---

### 3.4 Audio Types

**File:** `src/audio.rs`

```rust
/// Audio sample rate.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct SampleRate(pub u32);

impl SampleRate {
    pub fn hz(n: u32) -> Self;
    pub fn cd()  -> Self { Self(44100) }
    pub fn dvd() -> Self { Self(48000) }
    pub fn hd()  -> Self { Self(96000) }
}

/// Audio channel layout.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ChannelLayout {
    Mono,
    Stereo,
    Surround51,
    Surround71,
    Custom(u16),
}

impl ChannelLayout {
    pub fn channel_count(&self) -> u16;
}
```

---

### 3.5 Track Info

**File:** `src/track.rs`

```rust
/// A single track/stream within a media container.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Track {
    pub index: usize,
    pub kind: TrackKind,
    pub codec: Option<Codec>,
    pub bitrate: Option<u64>,
    pub language: Option<String>,
    pub is_default: bool,
    pub title: Option<String>,
    pub duration: Option<Duration>,
    pub video: Option<VideoTrackInfo>,
    pub audio: Option<AudioTrackInfo>,
    pub subtitle: Option<SubtitleTrackInfo>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VideoTrackInfo {
    pub resolution: Resolution,
    pub frame_rate: Option<FrameRate>,
    pub pixel_format: Option<String>,
    pub rotation: Option<i16>,
    pub color_space: Option<String>,
    pub bit_depth: Option<u8>,
    pub hdr: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AudioTrackInfo {
    pub sample_rate: SampleRate,
    pub channels: ChannelLayout,
    pub bit_depth: Option<u8>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SubtitleTrackInfo {
    pub format: String,
    pub forced: bool,
}
```

---

### 3.6 Codec — Open, Extensible Identifier

**File:** `src/codec.rs`

**Design rationale:** Codecs are an open set — new codecs appear regularly,
backends may support proprietary codecs, and users need custom identifiers.
A closed enum forces modification of core types to add anything.
Instead, `Codec` is a lightweight string ID with well-known constants.

```rust
use std::sync::Arc;

/// An open codec identifier.
///
/// Use well-known constants from the submodules (`codec::video::H264`,
/// `codec::audio::AAC`, etc.) or create custom identifiers.
///
/// # Examples
///
/// ```rust
/// use rskit_media::codec::{self, Codec, CodecKind};
///
/// let h264 = Codec::new(codec::video::H264);
/// let custom = Codec::new("my_proprietary_codec");
/// ```
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct Codec(Arc<str>);

impl Codec {
    pub fn new(id: impl Into<Arc<str>>) -> Self { Self(id.into()) }
    pub fn id(&self) -> &str { &self.0 }
}

impl std::fmt::Display for Codec {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}

/// Which domain a codec belongs to.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum CodecKind {
    Video,
    Audio,
    Image,
    Subtitle,
    Unknown,
}

/// Well-known video codecs.
pub mod video {
    pub const H264: &str = "h264";
    pub const H265: &str = "h265";
    pub const VP8: &str = "vp8";
    pub const VP9: &str = "vp9";
    pub const AV1: &str = "av1";
    pub const PRORES: &str = "prores";
    pub const MPEG2: &str = "mpeg2";
    pub const MPEG4: &str = "mpeg4";
    pub const THEORA: &str = "theora";
    pub const WMV3: &str = "wmv3";
}

/// Well-known audio codecs.
pub mod audio {
    pub const AAC: &str = "aac";
    pub const OPUS: &str = "opus";
    pub const MP3: &str = "mp3";
    pub const FLAC: &str = "flac";
    pub const VORBIS: &str = "vorbis";
    pub const PCM: &str = "pcm";
    pub const AC3: &str = "ac3";
    pub const EAC3: &str = "eac3";
    pub const WMA: &str = "wma";
    pub const ALAC: &str = "alac";
}

/// Well-known image codecs.
pub mod image {
    pub const PNG: &str = "png";
    pub const JPEG: &str = "jpeg";
    pub const WEBP: &str = "webp";
    pub const GIF: &str = "gif";
    pub const BMP: &str = "bmp";
    pub const TIFF: &str = "tiff";
    pub const AVIF: &str = "avif";
    pub const HEIF: &str = "heif";
}

/// Well-known subtitle codecs.
pub mod subtitle {
    pub const SRT: &str = "srt";
    pub const WEBVTT: &str = "webvtt";
    pub const ASS: &str = "ass";
    pub const SSA: &str = "ssa";
    pub const MOV_TEXT: &str = "mov_text";
}
```

---

### 3.7 Format — Open, Extensible Identifier

**File:** `src/format.rs`

Same rationale as Codec — container formats are an open set.

```rust
use std::sync::Arc;

/// An open container/file format identifier.
///
/// # Examples
///
/// ```rust
/// use rskit_media::format::{self, Format};
///
/// let mp4 = Format::new(format::MP4);
/// let custom = Format::new("my_container");
/// ```
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct Format(Arc<str>);

impl Format {
    pub fn new(id: impl Into<Arc<str>>) -> Self { Self(id.into()) }
    pub fn id(&self) -> &str { &self.0 }
}

impl std::fmt::Display for Format {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}

// ── Video containers ─────────────────────────────────────────────────
pub const MP4: &str = "mp4";
pub const MKV: &str = "mkv";
pub const WEBM: &str = "webm";
pub const AVI: &str = "avi";
pub const MOV: &str = "mov";
pub const FLV: &str = "flv";
pub const TS: &str = "ts";
pub const M4V: &str = "m4v";
pub const WMV: &str = "wmv";

// ── Audio containers ─────────────────────────────────────────────────
pub const MP3: &str = "mp3";
pub const WAV: &str = "wav";
pub const FLAC: &str = "flac";
pub const OGG: &str = "ogg";
pub const AAC: &str = "aac";
pub const M4A: &str = "m4a";
pub const WMA: &str = "wma";
pub const OPUS: &str = "opus";

// ── Image formats ────────────────────────────────────────────────────
pub const PNG: &str = "png";
pub const JPEG: &str = "jpeg";
pub const WEBP: &str = "webp";
pub const GIF: &str = "gif";
pub const BMP: &str = "bmp";
pub const TIFF: &str = "tiff";
pub const SVG: &str = "svg";
pub const AVIF: &str = "avif";
pub const HEIF: &str = "heif";

// ── Subtitle formats ─────────────────────────────────────────────────
pub const SRT: &str = "srt";
pub const VTT: &str = "vtt";
pub const ASS: &str = "ass";
```

---

### 3.8 Registry — Data-Driven Codec & Format Knowledge

**File:** `src/registry.rs`

**Design rationale:** Instead of hardcoded match arms for codec/format
compatibility, extension mapping, MIME types, etc., all knowledge lives
in a single data registry. Backends can register additional codecs.
Testing compatibility = looking up a table, not maintaining code paths.

```rust
/// Metadata about a codec.
#[derive(Debug, Clone)]
pub struct CodecInfo {
    pub id: Codec,
    pub kind: CodecKind,
    pub display_name: String,
    /// FFmpeg encoder name (e.g., "libx264", "libvpx-vp9").
    pub ffmpeg_encoder: Option<String>,
    /// FFmpeg decoder name (usually same as codec id).
    pub ffmpeg_decoder: Option<String>,
    /// Compatible container formats.
    pub compatible_formats: Vec<Format>,
}

/// Metadata about a container format.
#[derive(Debug, Clone)]
pub struct FormatInfo {
    pub id: Format,
    /// Default file extension (e.g., "mp4").
    pub extension: String,
    /// MIME type (e.g., "video/mp4").
    pub mime_type: String,
    /// Can hold multiple tracks?
    pub is_container: bool,
    /// What media types can this format hold?
    pub supported_media_types: Vec<MediaType>,
    /// Default video codec for this format.
    pub default_video_codec: Option<Codec>,
    /// Default audio codec for this format.
    pub default_audio_codec: Option<Codec>,
}

/// Central knowledge base for codec/format information and compatibility.
///
/// # Examples
///
/// ```rust
/// let registry = Registry::default();
///
/// let mp4 = Format::new(format::MP4);
/// let h264 = Codec::new(codec::video::H264);
///
/// assert!(registry.is_compatible(&h264, &mp4));
///
/// let info = registry.format_info(&mp4).unwrap();
/// assert_eq!(info.extension, "mp4");
/// assert_eq!(info.mime_type, "video/mp4");
/// ```
pub struct Registry {
    codecs: HashMap<Codec, CodecInfo>,
    formats: HashMap<Format, FormatInfo>,
}

impl Registry {
    /// Create a registry pre-loaded with all well-known codecs and formats.
    pub fn default() -> Self;

    /// Register a custom codec.
    pub fn register_codec(&mut self, info: CodecInfo);

    /// Register a custom format.
    pub fn register_format(&mut self, info: FormatInfo);

    /// Check if a codec is compatible with a format.
    pub fn is_compatible(&self, codec: &Codec, format: &Format) -> bool;

    /// Get the default codec pair for a format.
    pub fn default_codecs(&self, format: &Format) -> Option<(Codec, Codec)>;

    /// Look up codec metadata.
    pub fn codec_info(&self, codec: &Codec) -> Option<&CodecInfo>;

    /// Look up format metadata.
    pub fn format_info(&self, format: &Format) -> Option<&FormatInfo>;

    /// Detect format from a file extension.
    pub fn format_from_extension(&self, ext: &str) -> Option<&FormatInfo>;

    /// Detect format from a MIME type.
    pub fn format_from_mime(&self, mime: &str) -> Option<&FormatInfo>;

    /// List all registered codecs of a given kind.
    pub fn codecs_by_kind(&self, kind: CodecKind) -> Vec<&CodecInfo>;

    /// List all formats compatible with a given codec.
    pub fn formats_for_codec(&self, codec: &Codec) -> Vec<&FormatInfo>;
}
```

**Built-in compatibility data (loaded in `Registry::default()`):**

| Format | Video codecs | Audio codecs |
|--------|--------------|--------------|
| MP4 | h264, h265, av1 | aac, ac3, eac3, opus, mp3 |
| MKV | h264, h265, vp8, vp9, av1, * | aac, opus, flac, vorbis, * |
| WebM | vp8, vp9, av1 | opus, vorbis |
| AVI | h264, mpeg4, mpeg2 | mp3, pcm, ac3 |
| MOV | h264, h265, prores | aac, pcm, alac |
| TS | h264, h265, mpeg2 | aac, ac3, mp3 |
| MP3 | — | mp3 |
| WAV | — | pcm |
| FLAC | — | flac |
| OGG | theora | vorbis, opus, flac |

---

### 3.9 Filter — Extensible Processing Operations

**File:** `src/filter.rs`

**Design rationale:** Filters are an open set — FFmpeg alone has hundreds.
Instead of closed enums that need modification for every new filter,
`Filter` is an extensible struct with convenience constructors.

```rust
/// A named filter operation with typed parameters.
///
/// # Examples
///
/// ```rust
/// use rskit_media::filter::{self, filters};
///
/// // Use convenience constructors
/// let denoise = filters::denoise(3);
/// let sharpen = filters::sharpen(1.5);
/// let grayscale = filters::grayscale();
///
/// // Or build custom filters
/// let custom = filters::custom_video("chromakey=0x00FF00:0.1:0.2");
/// ```
#[derive(Debug, Clone)]
pub struct Filter {
    /// Filter name (maps to backend filter name, e.g., "hqdn3d" for denoise).
    pub name: String,
    /// Whether this filter targets video or audio stream.
    pub target: FilterTarget,
    /// Filter parameters.
    pub params: Params,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FilterTarget {
    Video,
    Audio,
}

/// Type-safe parameter map for filters.
#[derive(Debug, Clone, Default)]
pub struct Params(HashMap<String, ParamValue>);

impl Params {
    pub fn new() -> Self;
    pub fn set(self, key: impl Into<String>, val: impl Into<ParamValue>) -> Self;
    pub fn get(&self, key: &str) -> Option<&ParamValue>;
    pub fn iter(&self) -> impl Iterator<Item = (&str, &ParamValue)>;
}

#[derive(Debug, Clone)]
pub enum ParamValue {
    Int(i64),
    Float(f64),
    Str(String),
    Bool(bool),
}

impl From<i64> for ParamValue { ... }
impl From<f64> for ParamValue { ... }
impl From<String> for ParamValue { ... }
impl From<&str> for ParamValue { ... }
impl From<bool> for ParamValue { ... }

/// Convenience constructors for well-known filters.
///
/// Adding a new filter = one function here. Zero changes to core types.
pub mod filters {
    use super::*;

    // ── Video filters ────────────────────────────────────────────────

    pub fn denoise(strength: u8) -> Filter {
        Filter {
            name: "denoise".into(),
            target: FilterTarget::Video,
            params: Params::new().set("strength", strength as i64),
        }
    }

    pub fn sharpen(amount: f32) -> Filter {
        Filter {
            name: "sharpen".into(),
            target: FilterTarget::Video,
            params: Params::new().set("amount", amount as f64),
        }
    }

    pub fn blur(radius: f32) -> Filter {
        Filter {
            name: "blur".into(),
            target: FilterTarget::Video,
            params: Params::new().set("radius", radius as f64),
        }
    }

    pub fn brightness(value: f32) -> Filter {
        Filter {
            name: "brightness".into(),
            target: FilterTarget::Video,
            params: Params::new().set("value", value as f64),
        }
    }

    pub fn contrast(value: f32) -> Filter {
        Filter {
            name: "contrast".into(),
            target: FilterTarget::Video,
            params: Params::new().set("value", value as f64),
        }
    }

    pub fn saturation(value: f32) -> Filter {
        Filter {
            name: "saturation".into(),
            target: FilterTarget::Video,
            params: Params::new().set("value", value as f64),
        }
    }

    pub fn grayscale() -> Filter {
        Filter {
            name: "grayscale".into(),
            target: FilterTarget::Video,
            params: Params::new(),
        }
    }

    pub fn sepia() -> Filter {
        Filter {
            name: "sepia".into(),
            target: FilterTarget::Video,
            params: Params::new(),
        }
    }

    pub fn stabilize() -> Filter {
        Filter {
            name: "stabilize".into(),
            target: FilterTarget::Video,
            params: Params::new(),
        }
    }

    pub fn deinterlace() -> Filter {
        Filter {
            name: "deinterlace".into(),
            target: FilterTarget::Video,
            params: Params::new(),
        }
    }

    /// Pass a raw FFmpeg video filter string (e.g., "chromakey=0x00FF00:0.1:0.2").
    pub fn custom_video(raw: impl Into<String>) -> Filter {
        Filter {
            name: raw.into(),
            target: FilterTarget::Video,
            params: Params::new(),
        }
    }

    // ── Audio filters ────────────────────────────────────────────────

    pub fn high_pass(freq_hz: u32) -> Filter {
        Filter {
            name: "high_pass".into(),
            target: FilterTarget::Audio,
            params: Params::new().set("frequency", freq_hz as i64),
        }
    }

    pub fn low_pass(freq_hz: u32) -> Filter {
        Filter {
            name: "low_pass".into(),
            target: FilterTarget::Audio,
            params: Params::new().set("frequency", freq_hz as i64),
        }
    }

    pub fn equalizer(freq: u32, width: f32, gain: f32) -> Filter {
        Filter {
            name: "equalizer".into(),
            target: FilterTarget::Audio,
            params: Params::new()
                .set("frequency", freq as i64)
                .set("width", width as f64)
                .set("gain", gain as f64),
        }
    }

    pub fn noise_reduction(amount: f32) -> Filter {
        Filter {
            name: "noise_reduction".into(),
            target: FilterTarget::Audio,
            params: Params::new().set("amount", amount as f64),
        }
    }

    pub fn compressor(threshold: f32, ratio: f32) -> Filter {
        Filter {
            name: "compressor".into(),
            target: FilterTarget::Audio,
            params: Params::new()
                .set("threshold", threshold as f64)
                .set("ratio", ratio as f64),
        }
    }

    /// Pass a raw FFmpeg audio filter string.
    pub fn custom_audio(raw: impl Into<String>) -> Filter {
        Filter {
            name: raw.into(),
            target: FilterTarget::Audio,
            params: Params::new(),
        }
    }
}
```

---

### 3.10 Output Configuration

**File:** `src/output.rs`

```rust
/// Encoding quality preset.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Quality {
    Lossless,
    UltraHigh,
    High,
    Medium,
    Low,
    VeryLow,
    Custom(u8),
}

/// Bitrate specification.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Bitrate {
    Constant(u64),
    Variable(u64),
    Constrained { target: u64, max: u64 },
}

/// Encoding speed/effort tradeoff.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum EncodingSpeed {
    UltraFast,
    SuperFast,
    VeryFast,
    Fast,
    Medium,
    Slow,
    VerySlow,
}

/// Video-specific encoding settings.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VideoSettings {
    pub codec: Codec,
    pub resolution: Option<Resolution>,
    pub frame_rate: Option<FrameRate>,
    pub quality: Option<Quality>,
    pub bitrate: Option<Bitrate>,
    pub speed: Option<EncodingSpeed>,
}

impl VideoSettings {
    pub fn new(codec: Codec) -> Self;

    #[must_use] pub fn with_resolution(self, res: Resolution) -> Self;
    #[must_use] pub fn with_frame_rate(self, fps: FrameRate) -> Self;
    #[must_use] pub fn with_quality(self, q: Quality) -> Self;
    #[must_use] pub fn with_bitrate(self, br: Bitrate) -> Self;
    #[must_use] pub fn with_speed(self, speed: EncodingSpeed) -> Self;
}

/// Audio-specific encoding settings.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AudioSettings {
    pub codec: Codec,
    pub sample_rate: Option<SampleRate>,
    pub channels: Option<ChannelLayout>,
    pub bitrate: Option<Bitrate>,
}

impl AudioSettings {
    pub fn new(codec: Codec) -> Self;

    #[must_use] pub fn with_sample_rate(self, sr: SampleRate) -> Self;
    #[must_use] pub fn with_channels(self, ch: ChannelLayout) -> Self;
    #[must_use] pub fn with_bitrate(self, br: Bitrate) -> Self;
}

/// Complete output configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OutputConfig {
    pub format: Format,
    pub video: Option<VideoSettings>,
    pub audio: Option<AudioSettings>,
    pub strip_metadata: bool,
    pub extra: HashMap<String, String>,
}

impl OutputConfig {
    pub fn new(format: Format) -> Self;

    #[must_use] pub fn with_video(self, video: VideoSettings) -> Self;
    #[must_use] pub fn with_audio(self, audio: AudioSettings) -> Self;
    #[must_use] pub fn with_strip_metadata(self) -> Self;
    #[must_use] pub fn with_param(self, key: impl Into<String>, val: impl Into<String>) -> Self;

    /// Validate codec/format compatibility against a registry.
    pub fn validate(&self, registry: &Registry) -> AppResult<()>;
}
```

---

### 3.11 Presets — Common Output Configurations

**File:** `src/presets.rs`

Presets are separate from `OutputConfig` — they're convenience functions,
not core API. Adding project-specific presets means adding functions here,
not touching `OutputConfig`.

```rust
use crate::{codec, format, Format, Codec, OutputConfig, VideoSettings, AudioSettings, Quality};

// ── Video presets ────────────────────────────────────────────────────

pub fn mp4_h264() -> OutputConfig {
    OutputConfig::new(Format::new(format::MP4))
        .with_video(VideoSettings::new(Codec::new(codec::video::H264))
            .with_quality(Quality::Medium))
        .with_audio(AudioSettings::new(Codec::new(codec::audio::AAC)))
}

pub fn mp4_h265() -> OutputConfig {
    OutputConfig::new(Format::new(format::MP4))
        .with_video(VideoSettings::new(Codec::new(codec::video::H265))
            .with_quality(Quality::Medium))
        .with_audio(AudioSettings::new(Codec::new(codec::audio::AAC)))
}

pub fn webm_vp9() -> OutputConfig {
    OutputConfig::new(Format::new(format::WEBM))
        .with_video(VideoSettings::new(Codec::new(codec::video::VP9)))
        .with_audio(AudioSettings::new(Codec::new(codec::audio::OPUS)))
}

pub fn webm_av1() -> OutputConfig {
    OutputConfig::new(Format::new(format::WEBM))
        .with_video(VideoSettings::new(Codec::new(codec::video::AV1)))
        .with_audio(AudioSettings::new(Codec::new(codec::audio::OPUS)))
}

pub fn mkv_h265() -> OutputConfig {
    OutputConfig::new(Format::new(format::MKV))
        .with_video(VideoSettings::new(Codec::new(codec::video::H265)))
        .with_audio(AudioSettings::new(Codec::new(codec::audio::AAC)))
}

// ── Audio-only presets ───────────────────────────────────────────────

pub fn mp3() -> OutputConfig {
    OutputConfig::new(Format::new(format::MP3))
        .with_audio(AudioSettings::new(Codec::new(codec::audio::MP3)))
}

pub fn wav() -> OutputConfig {
    OutputConfig::new(Format::new(format::WAV))
        .with_audio(AudioSettings::new(Codec::new(codec::audio::PCM)))
}

pub fn flac() -> OutputConfig {
    OutputConfig::new(Format::new(format::FLAC))
        .with_audio(AudioSettings::new(Codec::new(codec::audio::FLAC)))
}

pub fn ogg_opus() -> OutputConfig {
    OutputConfig::new(Format::new(format::OGG))
        .with_audio(AudioSettings::new(Codec::new(codec::audio::OPUS)))
}

// ── Image presets ────────────────────────────────────────────────────

pub fn png() -> OutputConfig {
    OutputConfig::new(Format::new(format::PNG))
}

pub fn jpeg() -> OutputConfig {
    OutputConfig::new(Format::new(format::JPEG))
}

pub fn webp() -> OutputConfig {
    OutputConfig::new(Format::new(format::WEBP))
}

pub fn gif() -> OutputConfig {
    OutputConfig::new(Format::new(format::GIF))
}
```

---

### 3.12 MediaProbe Trait

**File:** `src/probe.rs`

```rust
/// Full probe result — everything knowable about a media file.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MediaMetadata {
    pub media_type: MediaType,
    pub format: Format,
    pub duration: Option<Duration>,
    pub size: Option<u64>,
    pub bitrate: Option<u64>,
    pub tracks: Vec<Track>,
    pub tags: HashMap<String, String>,
    pub created_at: Option<DateTime<Utc>>,
}

impl MediaMetadata {
    pub fn video_track(&self) -> Option<&Track>;
    pub fn audio_track(&self) -> Option<&Track>;
    pub fn subtitle_tracks(&self) -> Vec<&Track>;
    pub fn resolution(&self) -> Option<Resolution>;
    pub fn frame_rate(&self) -> Option<FrameRate>;
    pub fn sample_rate(&self) -> Option<SampleRate>;
    pub fn has_video(&self) -> bool;
    pub fn has_audio(&self) -> bool;
}

/// Inspect media files — extract metadata without processing.
#[async_trait]
pub trait MediaProbe: Send + Sync {
    async fn probe(&self, source: &FileSource) -> AppResult<MediaMetadata>;

    async fn thumbnail(
        &self,
        source: &FileSource,
        at: Timestamp,
        resolution: Option<Resolution>,
    ) -> AppResult<FileSource>;

    async fn thumbnails(
        &self,
        source: &FileSource,
        interval: Duration,
        resolution: Option<Resolution>,
    ) -> AppResult<Vec<FileSource>>;
}
```

---

### 3.13 Operation Types

The pipeline is built from a sequence of `MediaOp` values.
Each operation is a data-only description — no execution logic.

Detail structs are split by domain into submodules.

**File:** `src/ops/mod.rs`

```rust
mod spatial;
mod compose;

pub use spatial::*;
pub use compose::*;

use crate::{
    Filter, OutputConfig, SubtitleTrack, TimeRange, Segment, TrackKind,
};
use std::time::Duration;

/// A single media operation in a pipeline.
#[derive(Debug, Clone)]
pub enum MediaOp {
    // ── Temporal ─────────────────────────────────────────────────────
    Extract(TimeRange),
    ExtractMany(Vec<Segment>),

    // ── Spatial (video/image) ────────────────────────────────────────
    Resize(ResizeOp),
    Crop(CropRegion),
    Rotate(Rotation),
    Flip(FlipDirection),
    Pad(PadOp),

    // ── Speed / Time ─────────────────────────────────────────────────
    Speed(f64),
    Reverse,

    // ── Audio ────────────────────────────────────────────────────────
    Volume(f64),
    NormalizeAudio,
    FadeIn(Duration),
    FadeOut(Duration),
    StripAudio,
    StripVideo,

    // ── Filter (extensible — replaces closed VideoFilter/AudioFilter enums)
    Filter(Filter),

    // ── Composition ──────────────────────────────────────────────────
    Overlay(OverlayOp),
    Concat(ConcatOp),
    ReplaceAudio(ReplaceAudioOp),
    MixAudio(MixAudioOp),
    BurnSubtitles(SubtitleTrack),

    // ── Track selection ──────────────────────────────────────────────
    SelectTracks(Vec<usize>),
    SelectTracksByKind(Vec<TrackKind>),

    // ── Output ───────────────────────────────────────────────────────
    Transcode(OutputConfig),
}
```

**File:** `src/ops/spatial.rs`

```rust
use crate::Resolution;

#[derive(Debug, Clone)]
pub struct ResizeOp {
    pub resolution: Resolution,
    pub mode: ResizeMode,
}

#[derive(Debug, Clone, Copy)]
pub enum ResizeMode {
    Exact,
    Fit,
    Fill,
    FitWidth,
    FitHeight,
}

#[derive(Debug, Clone)]
pub struct CropRegion {
    pub x: u32,
    pub y: u32,
    pub width: u32,
    pub height: u32,
}

impl CropRegion {
    pub fn new(x: u32, y: u32, w: u32, h: u32) -> Self;
    pub fn center_aspect(source_res: Resolution, aspect_w: u32, aspect_h: u32) -> Self;
    pub fn center(source_res: Resolution, w: u32, h: u32) -> Self;
}

#[derive(Debug, Clone, Copy)]
pub enum Rotation {
    Degrees90,
    Degrees180,
    Degrees270,
    Arbitrary(f64),
}

#[derive(Debug, Clone, Copy)]
pub enum FlipDirection {
    Horizontal,
    Vertical,
    Both,
}

#[derive(Debug, Clone)]
pub struct PadOp {
    pub width: u32,
    pub height: u32,
    pub color: String,
}
```

**File:** `src/ops/compose.rs`

```rust
use crate::{FileSource, Timestamp, TimeRange};
use std::time::Duration;

#[derive(Debug, Clone)]
pub struct OverlayOp {
    pub source: FileSource,
    pub position: OverlayPosition,
    pub opacity: f32,
    pub time_range: Option<TimeRange>,
    pub scale: Option<f64>,
}

#[derive(Debug, Clone)]
pub enum OverlayPosition {
    TopLeft(u32, u32),
    TopRight(u32, u32),
    BottomLeft(u32, u32),
    BottomRight(u32, u32),
    Center,
    Custom { x: String, y: String },
}

#[derive(Debug, Clone)]
pub struct ConcatOp {
    pub source: FileSource,
    pub transition: Option<Transition>,
}

#[derive(Debug, Clone)]
pub enum Transition {
    CrossFade(Duration),
    FadeToBlack(Duration),
    Cut,
}

#[derive(Debug, Clone)]
pub struct ReplaceAudioOp {
    pub audio_source: FileSource,
    pub offset: Option<Timestamp>,
}

#[derive(Debug, Clone)]
pub struct MixAudioOp {
    pub audio_source: FileSource,
    pub volume: f64,
    pub offset: Option<Timestamp>,
}
```

---

### 3.14 MediaPipeline — Chainable Stream Processing

**File:** `src/pipeline.rs`

This is the core abstraction — a lazy, chainable pipeline that builds
an operation graph and compiles it into optimized backend commands.

```rust
/// A lazy pipeline of media operations.
///
/// Operations are not executed when chained — they are recorded.
/// Call `.execute()` to compile and run the full pipeline.
///
/// # Example
///
/// ```rust
/// use rskit_media::{presets, filter::filters};
///
/// let result = MediaPipeline::from(&source)
///     .extract(TimeRange::from_seconds(10.0, 60.0))
///     .resize(Resolution::p1080(), ResizeMode::Fit)
///     .filter(filters::denoise(3))
///     .volume(0.8)
///     .speed(1.25)
///     .transcode(presets::mp4_h264())
///     .execute(&executor)
///     .await?;
/// ```
pub struct MediaPipeline {
    source: FileSource,
    ops: Vec<MediaOp>,
    sink: Option<FileSink>,
}

impl MediaPipeline {
    // ── Construction ─────────────────────────────────────────────────

    pub fn from(source: &FileSource) -> Self;
    pub fn from_stored(store: &dyn FileStore, key: &str) -> Self;

    // ── Temporal ─────────────────────────────────────────────────────

    #[must_use]
    pub fn extract(self, range: TimeRange) -> Self;
    #[must_use]
    pub fn extract_many(self, segments: Vec<Segment>) -> Self;

    // ── Spatial ──────────────────────────────────────────────────────

    #[must_use]
    pub fn resize(self, resolution: Resolution, mode: ResizeMode) -> Self;
    #[must_use]
    pub fn crop(self, region: CropRegion) -> Self;
    #[must_use]
    pub fn rotate(self, rotation: Rotation) -> Self;
    #[must_use]
    pub fn flip(self, direction: FlipDirection) -> Self;
    #[must_use]
    pub fn pad(self, width: u32, height: u32, color: &str) -> Self;

    // ── Speed ────────────────────────────────────────────────────────

    #[must_use]
    pub fn speed(self, factor: f64) -> Self;
    #[must_use]
    pub fn reverse(self) -> Self;

    // ── Audio ────────────────────────────────────────────────────────

    #[must_use]
    pub fn volume(self, factor: f64) -> Self;
    #[must_use]
    pub fn normalize_audio(self) -> Self;
    #[must_use]
    pub fn fade_in(self, duration: Duration) -> Self;
    #[must_use]
    pub fn fade_out(self, duration: Duration) -> Self;
    #[must_use]
    pub fn strip_audio(self) -> Self;
    #[must_use]
    pub fn strip_video(self) -> Self;

    // ── Filters (extensible — pass any Filter) ───────────────────────

    #[must_use]
    pub fn filter(self, filter: Filter) -> Self;

    // ── Composition ──────────────────────────────────────────────────

    #[must_use]
    pub fn overlay(self, source: &FileSource, position: OverlayPosition, opacity: f32) -> Self;
    #[must_use]
    pub fn concat(self, source: &FileSource) -> Self;
    #[must_use]
    pub fn concat_with_transition(self, source: &FileSource, transition: Transition) -> Self;
    #[must_use]
    pub fn replace_audio(self, audio: &FileSource) -> Self;
    #[must_use]
    pub fn mix_audio(self, audio: &FileSource, volume: f64) -> Self;
    #[must_use]
    pub fn burn_subtitles(self, subs: SubtitleTrack) -> Self;

    // ── Track selection ──────────────────────────────────────────────

    #[must_use]
    pub fn select_tracks(self, indices: Vec<usize>) -> Self;
    #[must_use]
    pub fn select_tracks_by_kind(self, kinds: Vec<TrackKind>) -> Self;

    // ── Output ───────────────────────────────────────────────────────

    #[must_use]
    pub fn transcode(self, config: OutputConfig) -> Self;
    #[must_use]
    pub fn output_to(self, sink: FileSink) -> Self;

    // ── Execution ────────────────────────────────────────────────────

    pub async fn execute(self, executor: &dyn MediaExecutor) -> AppResult<FileSource>;

    pub async fn execute_with_progress(
        self,
        executor: &dyn MediaExecutor,
        on_progress: impl Fn(Progress) + Send + Sync + 'static,
    ) -> AppResult<FileSource>;

    // ── Inspection ───────────────────────────────────────────────────

    pub fn operations(&self) -> &[MediaOp];
    pub fn estimated_duration(&self, source_duration: Duration) -> Duration;
}

/// Execution progress report.
#[derive(Debug, Clone)]
pub struct Progress {
    pub position: Option<Timestamp>,
    pub total: Option<Duration>,
    pub percent: Option<f32>,
    pub speed: Option<f64>,
    pub output_size: Option<u64>,
    pub eta: Option<Duration>,
}
```

---

### 3.15 MediaExecutor Trait

**File:** `src/executor.rs`

```rust
/// Backend that can execute a media pipeline.
#[async_trait]
pub trait MediaExecutor: Send + Sync {
    async fn execute(
        &self,
        source: &FileSource,
        ops: &[MediaOp],
        sink: Option<&FileSink>,
    ) -> AppResult<FileSource>;

    async fn execute_with_progress(
        &self,
        source: &FileSource,
        ops: &[MediaOp],
        sink: Option<&FileSink>,
        on_progress: Box<dyn Fn(Progress) + Send + Sync>,
    ) -> AppResult<FileSource>;

    fn supports(&self, op: &MediaOp) -> bool;

    /// Dry run: return the command(s) that would be executed.
    fn preview(&self, source: &FileSource, ops: &[MediaOp]) -> AppResult<Vec<String>>;
}
```

---

### 3.16 Subtitle Types

**File:** `src/subtitle.rs`

```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SubtitleEntry {
    pub range: TimeRange,
    pub text: String,
    pub style: Option<SubtitleStyle>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SubtitleStyle {
    pub font_family: Option<String>,
    pub font_size: Option<u16>,
    pub color: Option<String>,
    pub background: Option<String>,
    pub bold: bool,
    pub italic: bool,
    pub position: SubtitlePosition,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub enum SubtitlePosition {
    Bottom,
    Top,
    Center,
    Custom { x: u32, y: u32 },
}

impl Default for SubtitlePosition {
    fn default() -> Self { Self::Bottom }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SubtitleTrack {
    pub entries: Vec<SubtitleEntry>,
    pub language: Option<String>,
    pub default_style: Option<SubtitleStyle>,
}

impl SubtitleTrack {
    pub fn new() -> Self;
    pub fn add(self, range: TimeRange, text: impl Into<String>) -> Self;
    pub fn with_language(self, lang: impl Into<String>) -> Self;
    pub fn from_srt(content: &str) -> AppResult<Self>;
    pub fn from_vtt(content: &str) -> AppResult<Self>;
    pub fn to_srt(&self) -> String;
    pub fn to_vtt(&self) -> String;
    pub fn shift(&mut self, offset: i64);
    pub fn in_range(&self, range: &TimeRange) -> Self;
}
```

---

## 4. `rskit-media-ffmpeg` — FFmpeg Backend

**What it does:** Implements `MediaProbe` and `MediaExecutor` by shelling out
to `ffprobe` and `ffmpeg` CLI tools. Compiles a pipeline's operation list
into optimized FFmpeg commands with `filter_complex`.

**Location:** `crates/rskit-media-ffmpeg/`

**Key dependencies:**

```toml
[dependencies]
rskit-errors = { path = "../rskit-errors" }
rskit-file   = { path = "../rskit-file" }
rskit-media  = { path = "../rskit-media" }
tokio        = { workspace = true, features = ["process", "io-util"] }
serde        = { workspace = true }
serde_json   = { workspace = true }
tracing      = { workspace = true }
which        = "7"
```

### Module Structure

```
src/
├── lib.rs              # mod declarations + pub use re-exports
├── config.rs           # FfmpegConfig, FfmpegLogLevel
├── probe.rs            # FfmpegProbe (impl MediaProbe)
├── executor.rs         # FfmpegExecutor (impl MediaExecutor)
├── command.rs          # FfmpegCommand, FfmpegInput (filter graph compilation)
├── filter_map.rs       # Filter name → FFmpeg filter string mapping
├── progress.rs         # FfmpegProgressParser
├── hw_accel.rs         # HwAccel enum + detection
```

**`src/lib.rs`:**

```rust
mod config;
mod probe;
mod executor;
mod command;
mod filter_map;
mod progress;
mod hw_accel;

pub use config::{FfmpegConfig, FfmpegLogLevel};
pub use probe::FfmpegProbe;
pub use executor::FfmpegExecutor;
pub use hw_accel::HwAccel;
```

---

### 4.1 Configuration

**File:** `src/config.rs`

```rust
#[derive(Debug, Clone, Deserialize)]
pub struct FfmpegConfig {
    pub ffmpeg_path: Option<PathBuf>,
    pub ffprobe_path: Option<PathBuf>,
    pub temp_dir: Option<PathBuf>,
    pub threads: Option<u32>,
    pub hw_accel: Option<HwAccel>,
    pub timeout: Option<Duration>,
    pub overwrite: bool,
    pub log_level: FfmpegLogLevel,
}

impl Default for FfmpegConfig {
    fn default() -> Self {
        Self {
            ffmpeg_path: None,
            ffprobe_path: None,
            temp_dir: None,
            threads: None,
            hw_accel: None,
            timeout: None,
            overwrite: true,
            log_level: FfmpegLogLevel::Warning,
        }
    }
}

#[derive(Debug, Clone, Copy, Deserialize)]
pub enum FfmpegLogLevel {
    Quiet,
    Panic,
    Fatal,
    Error,
    Warning,
    Info,
    Verbose,
    Debug,
}
```

---

### 4.2 FfmpegProbe

**File:** `src/probe.rs`

```rust
pub struct FfmpegProbe {
    config: FfmpegConfig,
}

impl FfmpegProbe {
    pub fn new(config: FfmpegConfig) -> Self;
    pub async fn check_available(&self) -> AppResult<String>;
    pub async fn probe_raw(&self, source: &FileSource) -> AppResult<serde_json::Value>;
}

#[async_trait]
impl MediaProbe for FfmpegProbe {
    async fn probe(&self, source: &FileSource) -> AppResult<MediaMetadata>;
    async fn thumbnail(
        &self, source: &FileSource, at: Timestamp, resolution: Option<Resolution>,
    ) -> AppResult<FileSource>;
    async fn thumbnails(
        &self, source: &FileSource, interval: Duration, resolution: Option<Resolution>,
    ) -> AppResult<Vec<FileSource>>;
}
```

**Implementation notes:**
- Runs `ffprobe -v quiet -print_format json -show_format -show_streams <input>`
- Parses JSON into `MediaMetadata`; codec strings from ffprobe map directly to `Codec::new()`
- Format strings map to `Format::new()`
- Thumbnail: `ffmpeg -ss <time> -i <input> -vframes 1 -vf scale=<w>:<h> <output.jpg>`
- URL sources are passed directly to ffprobe (supports HTTP/HTTPS)

---

### 4.3 FfmpegExecutor

**File:** `src/executor.rs`

```rust
pub struct FfmpegExecutor {
    config: FfmpegConfig,
    registry: Registry,
}

impl FfmpegExecutor {
    pub fn new(config: FfmpegConfig, registry: Registry) -> Self;
    pub async fn check_available(&self) -> AppResult<String>;
}

#[async_trait]
impl MediaExecutor for FfmpegExecutor {
    async fn execute(
        &self, source: &FileSource, ops: &[MediaOp], sink: Option<&FileSink>,
    ) -> AppResult<FileSource>;

    async fn execute_with_progress(
        &self, source: &FileSource, ops: &[MediaOp], sink: Option<&FileSink>,
        on_progress: Box<dyn Fn(Progress) + Send + Sync>,
    ) -> AppResult<FileSource>;

    fn supports(&self, op: &MediaOp) -> bool;
    fn preview(&self, source: &FileSource, ops: &[MediaOp]) -> AppResult<Vec<String>>;
}
```

---

### 4.4 Filter Graph Compilation

**File:** `src/command.rs`

The key internal engine: translating `Vec<MediaOp>` into FFmpeg CLI arguments.

```rust
struct FfmpegCommand {
    inputs: Vec<FfmpegInput>,
    video_filters: Vec<String>,
    audio_filters: Vec<String>,
    output_opts: Vec<String>,
    complex_filter: Option<String>,
    global_opts: Vec<String>,
}

struct FfmpegInput {
    source: FileSource,
    seek_to: Option<Timestamp>,
    duration: Option<Duration>,
}

impl FfmpegCommand {
    fn compile(
        source: &FileSource,
        ops: &[MediaOp],
        sink: Option<&FileSink>,
        config: &FfmpegConfig,
        registry: &Registry,
    ) -> AppResult<Self>;

    fn to_args(&self) -> Vec<String>;

    async fn run(
        &self,
        on_progress: Option<Box<dyn Fn(Progress) + Send + Sync>>,
    ) -> AppResult<FileSource>;
}
```

**Compilation rules:**

| Operation | FFmpeg mapping |
|---|---|
| `Extract(range)` | `-ss <start> -t <duration>` (input seeking) |
| `ExtractMany(segments)` | Multiple passes with concat demuxer |
| `Resize(res, Fit)` | `-vf scale=w:h:force_original_aspect_ratio=decrease,pad=w:h:(ow-iw)/2:(oh-ih)/2` |
| `Resize(res, Fill)` | `-vf scale=w:h:force_original_aspect_ratio=increase,crop=w:h` |
| `Resize(res, Exact)` | `-vf scale=w:h` |
| `Crop(region)` | `-vf crop=w:h:x:y` |
| `Rotate(90)` | `-vf transpose=1` |
| `Rotate(180)` | `-vf hflip,vflip` |
| `Speed(factor)` | `-vf setpts=PTS/{factor} -af atempo={factor}` |
| `Volume(f)` | `-af volume={f}` |
| `NormalizeAudio` | `-af loudnorm` |
| `FadeIn(d)` | `-af afade=t=in:d={d} -vf fade=t=in:d={d}` |
| `StripAudio` | `-an` |
| `StripVideo` | `-vn` |
| `Filter(f)` | Looked up via `filter_map` → FFmpeg filter string |
| `Overlay(op)` | `-filter_complex [0][1]overlay=x:y` |
| `Concat(op)` | `-filter_complex [0][1]concat=n=2:v=1:a=1` |
| `ReplaceAudio(op)` | `-map 0:v -map 1:a` |
| `Transcode(config)` | Codec from registry → `-c:v <encoder> -c:a <encoder> -crf <q>` |

**Video filter chaining:** When multiple video filters are present, they are
joined into a single `-vf` string: `scale=1920:1080,crop=1920:800,hqdn3d=3`.

**Complex filter:** Multi-input operations (overlay, concat, mix) build a
`-filter_complex` graph with labeled pads.

---

### 4.5 Filter Mapping

**File:** `src/filter_map.rs`

Maps `Filter` names + params to FFmpeg filter strings. This is where
the extensible `Filter` type meets FFmpeg's specific syntax.

```rust
/// Convert a Filter into an FFmpeg filter string.
///
/// # Examples
///
/// `filters::denoise(3)` → `"hqdn3d=3"`
/// `filters::sharpen(1.5)` → `"unsharp=5:5:1.5"`
/// `filters::grayscale()` → `"format=gray"`
/// `filters::custom_video("chromakey=0x00FF00")` → `"chromakey=0x00FF00"`
pub fn to_ffmpeg_filter(filter: &Filter) -> String;

/// Map of well-known filter names to FFmpeg filter generators.
/// Custom/unknown filters are passed through as-is (their name IS the FFmpeg filter).
struct FilterMap { ... }

impl FilterMap {
    fn default() -> Self;
    fn register(&mut self, name: &str, generator: impl Fn(&Params) -> String);
    fn resolve(&self, filter: &Filter) -> String;
}
```

**Built-in mappings:**

| Filter name | FFmpeg output |
|---|---|
| `denoise` | `hqdn3d={strength}` |
| `sharpen` | `unsharp=5:5:{amount}` |
| `blur` | `boxblur={radius}` |
| `brightness` | `eq=brightness={value}` |
| `contrast` | `eq=contrast={value}` |
| `saturation` | `eq=saturation={value}` |
| `grayscale` | `format=gray` |
| `sepia` | `colorchannelmixer=.393:.769:.189:0:.349:.686:.168:0:.272:.534:.131` |
| `stabilize` | `vidstabdetect,vidstabtransform` |
| `deinterlace` | `yadif` |
| `high_pass` | `highpass=f={frequency}` |
| `low_pass` | `lowpass=f={frequency}` |
| `equalizer` | `equalizer=f={frequency}:width_type=h:width={width}:gain={gain}` |
| `noise_reduction` | `afftdn=nf=-{amount*25}` |
| `compressor` | `acompressor=threshold={threshold}:ratio={ratio}` |
| *(unknown)* | filter name passed through verbatim (allows any FFmpeg filter) |

---

### 4.6 Progress Reporting

**File:** `src/progress.rs`

```rust
struct FfmpegProgressParser {
    total_duration: Option<Duration>,
}

impl FfmpegProgressParser {
    fn new(total_duration: Option<Duration>) -> Self;
    fn parse_line(&self, line: &str) -> Option<Progress>;
}
```

FFmpeg outputs lines like:
```
frame= 1234 fps=120 q=23.0 size=   12345kB time=00:01:23.45 bitrate=1234.5kbits/s speed=2.5x
```

Parser extracts `time=`, `speed=`, `size=` to build `Progress` reports.

---

### 4.7 Hardware Acceleration

**File:** `src/hw_accel.rs`

```rust
#[derive(Debug, Clone, Copy, Deserialize)]
pub enum HwAccel {
    VideoToolbox,
    Cuda,
    Qsv,
    Vaapi,
    Vulkan,
    D3d11va,
    Auto,
}

impl HwAccel {
    pub fn ffmpeg_arg(&self) -> &str;
    pub async fn detect_available() -> Vec<HwAccel>;
}
```

---

## 5. `rskit-media-image` — Image Backend

### Why Separate from FFmpeg

For pure image operations (resize, crop, rotate, format conversion), the `image`
crate is faster, lighter, and doesn't require FFmpeg to be installed. This crate
handles image-only pipelines natively.

**Location:** `crates/rskit-media-image/`

**Key dependencies:**

```toml
[dependencies]
rskit-errors = { path = "../rskit-errors" }
rskit-file   = { path = "../rskit-file" }
rskit-media  = { path = "../rskit-media" }
image        = "0.25"
webp         = "0.3"
tracing      = { workspace = true }
async-trait  = { workspace = true }
```

### Module Structure

```
src/
├── lib.rs              # mod declarations + pub use re-exports
├── processor.rs        # ImageProcessor (impl MediaExecutor)
```

**`src/lib.rs`:**

```rust
mod processor;

pub use processor::ImageProcessor;
```

---

### 5.1 ImageProcessor

**File:** `src/processor.rs`

```rust
/// Image-specific executor using the `image` crate.
///
/// Handles image operations (Resize, Crop, Rotate, Flip, subset of Filters,
/// Transcode). Returns `Err(unsupported)` for video/audio operations.
pub struct ImageProcessor;

impl ImageProcessor {
    pub fn new() -> Self;
}

#[async_trait]
impl MediaExecutor for ImageProcessor {
    async fn execute(
        &self, source: &FileSource, ops: &[MediaOp], sink: Option<&FileSink>,
    ) -> AppResult<FileSource>;

    fn supports(&self, op: &MediaOp) -> bool;
    fn preview(&self, source: &FileSource, ops: &[MediaOp]) -> AppResult<Vec<String>>;
}
```

### 5.2 Supported Operations

| Operation | image crate mapping |
|---|---|
| `Resize(res, Fit)` | `imageops::resize` with `FilterType::Lanczos3` |
| `Resize(res, Fill)` | resize + center crop |
| `Resize(res, Exact)` | `imageops::resize` exact |
| `Crop(region)` | `imageops::crop` |
| `Rotate(90/180/270)` | `rotate90/180/270` |
| `Flip(Horizontal)` | `fliph` |
| `Flip(Vertical)` | `flipv` |
| `Filter("grayscale")` | `grayscale()` |
| `Filter("blur")` | `blur(radius)` |
| `Filter("brightness")` | `brighten(value)` |
| `Filter("contrast")` | `contrast(value)` |
| `Transcode(format)` | save as PNG/JPEG/WebP/BMP/GIF/TIFF |

---

## 6. Workspace Changes

### New workspace members

```toml
[workspace]
members = [
    # ... existing members ...
    "crates/rskit-file",
    "crates/rskit-media",
    "crates/rskit-media-ffmpeg",
    "crates/rskit-media-image",
]
```

### New workspace dependencies

```toml
# File I/O (rskit-file)
tempfile     = "3"
mime_guess   = "2"
infer        = "0.19"

# Cloud storage (rskit-file, optional)
aws-sdk-s3   = { version = "1", optional = true }
aws-config   = { version = "1", optional = true }

# Image processing (rskit-media-image)
image        = "0.25"
webp         = "0.3"

# FFmpeg binary lookup (rskit-media-ffmpeg)
which        = "7"
```

### `rskit` facade — new feature flags

```toml
[features]
# ... existing features ...
file          = ["dep:rskit-file"]
media         = ["dep:rskit-media", "file"]
media-ffmpeg  = ["dep:rskit-media-ffmpeg", "media"]
media-image   = ["dep:rskit-media-image", "media"]
media-full    = ["media-ffmpeg", "media-image"]
file-s3       = ["file", "rskit-file/s3"]
file-gcs      = ["file", "rskit-file/gcs"]
full          = [
    # ... existing ...
    "media-full", "file",
]
```

---

## 7. Dependency Reference

| New dep | Used by | Reason |
|---|---|---|
| `tempfile 3` | rskit-file | Managed temporary files with auto-cleanup |
| `mime_guess 2` | rskit-file | Extension → MIME type mapping |
| `infer 0.19` | rskit-file | Magic-bytes MIME detection |
| `aws-sdk-s3 1` | rskit-file (opt) | Amazon S3 storage backend |
| `aws-config 1` | rskit-file (opt) | AWS credential resolution |
| `google-cloud-storage` | rskit-file (opt) | GCS storage backend |
| `image 0.25` | rskit-media-image | Native image processing |
| `webp 0.3` | rskit-media-image | WebP encode/decode |
| `which 7` | rskit-media-ffmpeg | Locate ffmpeg/ffprobe binaries |

All dependencies use `rustls` where applicable (no OpenSSL).

---

## 8. Implementation Order

```
Phase 1 — rskit-file (no media knowledge)
  └─ FileSource, FileSink, TempFile, TempDir
  └─ FileMeta, FileKind, detect_mime, detect_kind
  └─ FileStore trait + LocalStore
  └─ S3Store (feature: s3), GcsStore (feature: gcs)

Phase 2 — rskit-media (pure types + traits, no processing)
  └─ types, time, spatial, audio (value types)
  └─ Codec + Format (open string IDs + constants)
  └─ Registry (data-driven compatibility)
  └─ Filter + filters module
  └─ OutputConfig + VideoSettings + AudioSettings + presets
  └─ Track, probe (MediaMetadata, MediaProbe)
  └─ ops/ (MediaOp + spatial + compose)
  └─ MediaPipeline, MediaExecutor, subtitle

Phase 3a — rskit-media-ffmpeg (video/audio backend)
  └─ FfmpegConfig
  └─ FfmpegProbe (implements MediaProbe)
  └─ filter_map (Filter → FFmpeg filter strings)
  └─ FfmpegCommand (filter graph compiler, uses registry)
  └─ FfmpegExecutor (implements MediaExecutor)
  └─ Progress parser
  └─ HwAccel detection

Phase 3b — rskit-media-image (image backend, parallel with 3a)
  └─ ImageProcessor (implements MediaExecutor for image ops)
```

### Crate count

| Phase | Crates | Status |
|---|---|---|
| Existing | 24 | Complete |
| Phase 1 | +1 (rskit-file) | In spec |
| Phase 2 | +1 (rskit-media) | In spec |
| Phase 3 | +2 (rskit-media-ffmpeg, rskit-media-image) | In spec |
| **Total** | **28** | |
