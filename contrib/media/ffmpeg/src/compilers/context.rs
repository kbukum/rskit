//! Shared context threaded through every per-operation compiler.

use crate::{
    command::{FfmpegCommand, SourceHints},
    config::FfmpegConfig,
};
use rskit_media::registry::Registry;

/// Context passed to each per-operation compiler.
pub(crate) struct CompileContext<'a> {
    pub cmd: &'a mut FfmpegCommand,
    pub config: &'a FfmpegConfig,
    pub hints: &'a SourceHints,
    pub registry: &'a Registry,
}
