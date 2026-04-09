//! Silence detection in audio data.
//!
//! Scans decoded samples and emits regions whose amplitude stays below a
//! configurable threshold for a minimum duration.

use crate::wav::WavReader;

/// Configuration for silence detection.
#[derive(Debug, Clone)]
pub struct SilenceConfig {
    /// Amplitude threshold — samples with `|s| ≤ threshold` are considered silent.
    /// Range: `0.0..=1.0`. Default: `0.01` (~-40 dB).
    pub threshold: f32,
    /// Minimum silence duration in seconds. Regions shorter than this are ignored.
    /// Default: `0.5`.
    pub min_duration_secs: f64,
}

impl Default for SilenceConfig {
    fn default() -> Self {
        Self {
            threshold: 0.01,
            min_duration_secs: 0.5,
        }
    }
}

/// A detected silence region.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct SilenceRegion {
    /// Start time in seconds.
    pub start_secs: f64,
    /// End time in seconds.
    pub end_secs: f64,
}

impl SilenceRegion {
    /// Duration of the silence region.
    #[must_use]
    pub fn duration_secs(&self) -> f64 {
        self.end_secs - self.start_secs
    }
}

/// Detect silence regions in decoded audio.
///
/// Mixes to mono before analysis. Returns regions sorted by start time.
pub fn detect_silence(wav: &WavReader, config: &SilenceConfig) -> Vec<SilenceRegion> {
    let ch = wav.spec.channels as usize;
    let sr = wav.spec.sample_rate as f64;
    let frames = wav.frame_count();

    if frames == 0 || sr == 0.0 {
        return Vec::new();
    }

    let threshold = config.threshold.abs();
    let min_frames = (config.min_duration_secs * sr) as usize;

    let mut regions = Vec::new();
    let mut silence_start: Option<usize> = None;

    for frame in 0..frames {
        // Peak across channels
        let mut peak: f32 = 0.0;
        for c in 0..ch {
            let idx = frame * ch + c;
            if idx < wav.samples.len() {
                let abs = wav.samples[idx].abs();
                if abs > peak {
                    peak = abs;
                }
            }
        }

        let is_silent = peak <= threshold;

        match (is_silent, silence_start) {
            (true, None) => {
                silence_start = Some(frame);
            }
            (false, Some(start)) => {
                let length = frame - start;
                if length >= min_frames {
                    regions.push(SilenceRegion {
                        start_secs: start as f64 / sr,
                        end_secs: frame as f64 / sr,
                    });
                }
                silence_start = None;
            }
            _ => {}
        }
    }

    // Handle silence at the end
    if let Some(start) = silence_start {
        let length = frames - start;
        if length >= min_frames {
            regions.push(SilenceRegion {
                start_secs: start as f64 / sr,
                end_secs: frames as f64 / sr,
            });
        }
    }

    regions
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::wav::WavSpec;

    fn mono_wav(sample_rate: u32, samples: Vec<f32>) -> WavReader {
        WavReader {
            spec: WavSpec {
                channels: 1,
                sample_rate,
                bits_per_sample: 16,
            },
            samples,
        }
    }

    #[test]
    fn detect_leading_silence() {
        let sr = 1000u32;
        // 1 second silence + 1 second tone
        let mut samples = vec![0.0f32; 1000];
        samples.extend(vec![0.5f32; 1000]);

        let wav = mono_wav(sr, samples);
        let regions = detect_silence(
            &wav,
            &SilenceConfig {
                threshold: 0.01,
                min_duration_secs: 0.5,
            },
        );

        assert_eq!(regions.len(), 1);
        assert!((regions[0].start_secs - 0.0).abs() < 0.01);
        assert!((regions[0].end_secs - 1.0).abs() < 0.01);
    }

    #[test]
    fn detect_trailing_silence() {
        let sr = 1000u32;
        let mut samples = vec![0.5f32; 1000];
        samples.extend(vec![0.0f32; 1000]);

        let wav = mono_wav(sr, samples);
        let regions = detect_silence(&wav, &SilenceConfig::default());

        assert_eq!(regions.len(), 1);
        assert!((regions[0].start_secs - 1.0).abs() < 0.01);
        assert!((regions[0].end_secs - 2.0).abs() < 0.01);
    }

    #[test]
    fn no_silence() {
        let sr = 1000u32;
        let samples = vec![0.5f32; 2000];
        let wav = mono_wav(sr, samples);
        let regions = detect_silence(&wav, &SilenceConfig::default());
        assert!(regions.is_empty());
    }

    #[test]
    fn short_silence_ignored() {
        let sr = 1000u32;
        let mut samples = vec![0.5f32; 500];
        samples.extend(vec![0.0f32; 100]); // 0.1s — below min 0.5s
        samples.extend(vec![0.5f32; 500]);

        let wav = mono_wav(sr, samples);
        let regions = detect_silence(&wav, &SilenceConfig::default());
        assert!(regions.is_empty());
    }
}
