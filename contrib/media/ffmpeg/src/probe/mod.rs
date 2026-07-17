//! FFmpeg probe implementation — media analysis via `ffprobe` and `ffmpeg`.
//!
//! Organized into focused sub-modules:
//! - [`parse`] — FFprobe JSON → [`MediaMetadata`](rskit_media::probe::MediaMetadata) conversion
//! - [`thumbnail`] — Thumbnail and visual extraction
//! - [`detect`] — Scene, keyframe, silence, and chapter detection

mod core;
mod detect;
mod media_probe;
mod parse;
#[cfg(test)]
mod tests;
mod thumbnail;

pub(crate) use core::FfmpegProbe;
