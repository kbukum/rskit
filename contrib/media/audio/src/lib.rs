//! Pure Rust audio processing — no FFmpeg dependency.
//!
//! Provides lightweight audio analysis and processing for common tasks:
//! - WAV file reading/writing
//! - Waveform generation (peak / RMS)
//! - Silence detection
//! - Loudness measurement (peak, RMS, EBU R128 approximation)
//! - Volume adjustment and fade effects
//!
//! For complex operations (encoding, format conversion, filters) use
//! [`rskit-media-ffmpeg`](../rskit_media_ffmpeg) instead.

#![warn(missing_docs)]

mod config;
mod loudness;
mod probe;
mod silence;
mod wav;
mod waveform;

#[cfg(test)]
mod tests;

pub use config::Config;
pub use probe::register;
