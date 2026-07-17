//! FFmpeg executor — concurrency-controlled media processing with hw accel fallback.

mod media_executor;
mod output;
mod resolve;
mod retry;
#[cfg(test)]
mod tests;
mod types;

pub(crate) use types::FfmpegExecutor;
