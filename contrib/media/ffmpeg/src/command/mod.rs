//! FFmpeg command builder — compiles MediaOp list into FFmpeg CLI arguments.

mod args;
mod compile;
mod optimize;
mod runner;
#[cfg(test)]
mod tests;
mod types;

pub(crate) use types::{FfmpegCommand, FfmpegInput, SourceHints};
