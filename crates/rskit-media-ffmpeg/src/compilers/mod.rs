//! Per-operation compilers that translate [`rskit_media::ops::MediaOp`] variants into FFmpeg arguments.

pub(crate) mod ai;
pub(crate) mod audio;
pub(crate) mod compose;
pub(crate) mod extract;
pub(crate) mod filter;
pub(crate) mod overlay;
pub(crate) mod scene_detect;
pub(crate) mod spatial;
pub(crate) mod subtitle;
pub(crate) mod temporal;
pub(crate) mod thumbnail;
pub(crate) mod tracks;
pub(crate) mod transcode;
pub(crate) mod visual;

use crate::command::{FfmpegCommand, SourceHints};
use rskit_media::registry::Registry;

/// Context passed to each per-operation compiler.
pub(crate) struct CompileContext<'a> {
    pub cmd: &'a mut FfmpegCommand,
    pub hints: &'a SourceHints,
    pub registry: &'a Registry,
}
