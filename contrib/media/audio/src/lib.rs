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

pub mod loudness;
pub mod silence;
pub mod wav;
pub mod waveform;

pub use loudness::{LoudnessInfo, LoudnessMeter};
pub use silence::{SilenceConfig, SilenceRegion, detect_silence};
pub use wav::{WavReader, WavSpec};
pub use waveform::{WaveformConfig, WaveformPoint, generate_waveform};
