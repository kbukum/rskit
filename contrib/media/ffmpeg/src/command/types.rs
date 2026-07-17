use std::time::Duration;

use rskit_media::time::Timestamp;
use rskit_storage::FileSource;

/// FFmpeg input specification (source file, optional seek/duration).
pub(crate) struct FfmpegInput {
    pub source: FileSource,
    pub seek_to: Option<Timestamp>,
    pub duration: Option<Duration>,
}

/// Optional hints about the source media, used to make smarter compilation decisions.
///
/// When available (e.g., from a prior probe), these hints let the command builder
/// generate more accurate filter graphs. Without hints, the builder uses conservative
/// defaults.
#[derive(Debug, Clone, Default)]
pub(crate) struct SourceHints {
    /// Whether the primary source has at least one audio stream.
    /// `None` means unknown — the builder will assume audio exists (common case).
    pub has_audio: Option<bool>,
}

/// Compiled FFmpeg command ready for execution.
///
/// Holds all inputs, filters, output options, and global flags. Use
/// [`compile`](FfmpegCommand::compile) to construct from media operations.
pub(crate) struct FfmpegCommand {
    /// Input file specifications.
    pub(crate) inputs: Vec<FfmpegInput>,
    /// Video filter expressions (joined with `,` into a `-vf` chain).
    pub(crate) video_filters: Vec<String>,
    /// Audio filter expressions (joined with `,` into an `-af` chain).
    pub(crate) audio_filters: Vec<String>,
    /// Additional output options (codec flags, maps, etc.).
    pub(crate) output_opts: Vec<String>,
    /// Complex filter graph (used for concat, overlay, etc.).
    pub(crate) complex_filter: Option<String>,
    /// Global options applied before inputs (`-y`, `-loglevel`, etc.).
    pub(crate) global_opts: Vec<String>,
    /// Temp files kept alive for the duration of the command (e.g., subtitle files).
    #[allow(dead_code)]
    pub(crate) temp_files: Vec<rskit_storage::TempFile>,
}
