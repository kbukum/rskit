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

mod loudness;
mod silence;
mod wav;
mod waveform;

use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;

use rskit_errors::{AppError, AppResult, ErrorCode};
use rskit_media::{
    AudioTrackInfo, ChannelLayout, Codec, Format, MediaMetadata, MediaProbe, MediaType, Registry,
    Resolution, SampleRate, SilenceInterval, Timestamp, Track, TrackKind, codec, format,
};
use rskit_storage::FileSource;
use tokio::io::AsyncReadExt;

use crate::loudness::LoudnessMeter;
use crate::silence::{SilenceConfig, detect_silence};
use crate::wav::WavReader;
use crate::waveform::{WaveformConfig, generate_waveform};

/// Configuration for the pure Rust audio backend.
#[derive(Debug, Clone)]
pub struct Config {
    /// Maximum source size read into memory while probing.
    pub max_probe_bytes: u64,
    /// Number of waveform bins summarized into metadata tags during probing.
    pub metadata_waveform_bins: usize,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            max_probe_bytes: 64 * 1024 * 1024,
            metadata_waveform_bins: 20,
        }
    }
}

impl Config {
    /// Override the maximum source size read into memory while probing.
    #[must_use]
    pub fn with_max_probe_bytes(mut self, max_probe_bytes: u64) -> Self {
        self.max_probe_bytes = max_probe_bytes;
        self
    }

    /// Override the waveform bin count summarized into metadata tags.
    #[must_use]
    pub fn with_metadata_waveform_bins(mut self, metadata_waveform_bins: usize) -> Self {
        self.metadata_waveform_bins = metadata_waveform_bins;
        self
    }
}

/// Register the audio backend.
pub fn register(registry: &mut Registry, config: Config) -> AppResult<()> {
    let config = Arc::new(config);
    registry.register_probe(
        "audio",
        Arc::new(move || {
            Ok(Arc::new(AudioProbe {
                config: Arc::clone(&config),
            }))
        }),
    )
}

struct AudioProbe {
    config: Arc<Config>,
}

#[async_trait::async_trait]
impl MediaProbe for AudioProbe {
    async fn probe(&self, source: &FileSource) -> AppResult<MediaMetadata> {
        let wav = self.read_wav(source).await?;
        Ok(metadata_for_wav(&wav, self.config.metadata_waveform_bins))
    }

    async fn thumbnail(
        &self,
        _source: &FileSource,
        _at: Timestamp,
        _resolution: Option<Resolution>,
    ) -> AppResult<FileSource> {
        unsupported("audio thumbnail extraction is not supported by the pure Rust audio backend")
    }

    async fn thumbnails(
        &self,
        _source: &FileSource,
        _interval: Duration,
        _resolution: Option<Resolution>,
    ) -> AppResult<Vec<FileSource>> {
        unsupported("audio thumbnail extraction is not supported by the pure Rust audio backend")
    }

    async fn silence_detect(
        &self,
        source: &FileSource,
        min_duration: Duration,
        noise_threshold_db: f64,
    ) -> AppResult<Vec<SilenceInterval>> {
        let wav = self.read_wav(source).await?;
        let threshold = 10f64.powf(noise_threshold_db / 20.0) as f32;
        let config = SilenceConfig {
            threshold,
            min_duration_secs: min_duration.as_secs_f64(),
        };

        Ok(detect_silence(&wav, &config)
            .into_iter()
            .map(|region| SilenceInterval {
                start: Timestamp::from_seconds(region.start_secs),
                end: Timestamp::from_seconds(region.end_secs),
                duration: Duration::from_secs_f64(region.duration_secs()),
            })
            .collect())
    }
}

impl AudioProbe {
    async fn read_wav(&self, source: &FileSource) -> AppResult<WavReader> {
        let data = read_bounded(source, self.config.max_probe_bytes).await?;
        WavReader::from_bytes(&data)
    }
}

async fn read_bounded(source: &FileSource, max_bytes: u64) -> AppResult<Vec<u8>> {
    let mut reader = source.reader().await?.take(max_bytes.saturating_add(1));
    let capacity = usize::try_from(max_bytes.min(1024 * 1024)).map_err(|_| {
        AppError::new(
            ErrorCode::InvalidInput,
            "audio probe byte limit does not fit in memory",
        )
    })?;
    let mut data = Vec::with_capacity(capacity);
    reader.read_to_end(&mut data).await.map_err(|error| {
        AppError::new(
            ErrorCode::Internal,
            format!("failed to read audio source: {error}"),
        )
    })?;
    if data.len() as u64 > max_bytes {
        return Err(AppError::new(
            ErrorCode::InvalidInput,
            format!("audio source exceeds probe limit of {max_bytes} bytes"),
        ));
    }
    Ok(data)
}

fn metadata_for_wav(wav: &WavReader, waveform_bins: usize) -> MediaMetadata {
    let duration = Duration::from_secs_f64(wav.duration_secs());
    let channels = channel_layout(wav.spec.channels);
    let bitrate = u64::from(wav.spec.sample_rate)
        .saturating_mul(u64::from(wav.spec.channels))
        .saturating_mul(u64::from(wav.spec.bits_per_sample));
    let loudness = LoudnessMeter::measure(wav);
    let waveform = generate_waveform(
        wav,
        &WaveformConfig {
            bins: waveform_bins,
            channel: None,
        },
    );

    let mut tags = HashMap::new();
    tags.insert("audio.peak".to_owned(), loudness.peak.to_string());
    tags.insert("audio.peak_db".to_owned(), loudness.peak_db.to_string());
    tags.insert("audio.rms".to_owned(), loudness.rms.to_string());
    tags.insert("audio.rms_db".to_owned(), loudness.rms_db.to_string());
    tags.insert("audio.lufs".to_owned(), loudness.lufs.to_string());
    tags.insert("audio.waveform_bins".to_owned(), waveform.len().to_string());
    if let Some(max_peak) = waveform.iter().map(|point| point.peak).reduce(f32::max) {
        tags.insert("audio.waveform_peak".to_owned(), max_peak.to_string());
    }
    if let Some(max_rms) = waveform.iter().map(|point| point.rms).reduce(f32::max) {
        tags.insert("audio.waveform_rms".to_owned(), max_rms.to_string());
    }
    if let Some(min_sample) = waveform.iter().map(|point| point.min).reduce(f32::min) {
        tags.insert("audio.waveform_min".to_owned(), min_sample.to_string());
    }
    if let Some(max_sample) = waveform.iter().map(|point| point.max).reduce(f32::max) {
        tags.insert("audio.waveform_max".to_owned(), max_sample.to_string());
    }

    MediaMetadata {
        media_type: MediaType::Audio,
        format: Format::new(format::WAV),
        duration: Some(duration),
        size: None,
        bitrate: Some(bitrate),
        tracks: vec![Track {
            index: 0,
            kind: TrackKind::Audio,
            codec: Some(Codec::new(codec::audio::PCM)),
            bitrate: Some(bitrate),
            language: None,
            is_default: true,
            title: None,
            duration: Some(duration),
            video: None,
            audio: Some(AudioTrackInfo {
                sample_rate: SampleRate::hz(wav.spec.sample_rate),
                channels,
                bit_depth: Some(wav.spec.bits_per_sample as u8),
            }),
            subtitle: None,
        }],
        tags,
        created_at: None,
    }
}

fn channel_layout(channels: u16) -> ChannelLayout {
    match channels {
        1 => ChannelLayout::Mono,
        2 => ChannelLayout::Stereo,
        6 => ChannelLayout::Surround51,
        8 => ChannelLayout::Surround71,
        channels => ChannelLayout::Custom(channels),
    }
}

fn unsupported<T>(message: &'static str) -> AppResult<T> {
    Err(AppError::new(ErrorCode::InvalidInput, message))
}
