//! Audio loudness measurement — peak, RMS, and approximate EBU R128.
//!
//! Provides quick loudness analysis on decoded samples without requiring external tools.

use crate::wav::WavReader;

/// Loudness measurement result.
#[derive(Debug, Clone, Copy)]
pub struct LoudnessInfo {
    /// Peak amplitude (linear, `0.0..=1.0`).
    pub peak: f32,
    /// Peak in dBFS.
    pub peak_db: f32,
    /// RMS amplitude (linear).
    pub rms: f32,
    /// RMS in dBFS.
    pub rms_db: f32,
    /// Approximate integrated loudness in LUFS (simplified EBU R128).
    ///
    /// This is a lightweight approximation — for broadcast-grade measurement use a dedicated R128 analyser
    /// or FFmpeg's `loudnorm` filter.
    pub lufs: f32,
}

/// Loudness meter that can analyse decoded audio.
pub struct LoudnessMeter;

impl LoudnessMeter {
    /// Measure loudness of a decoded WAV.
    ///
    /// Mixes all channels to mono for measurement.
    pub fn measure(wav: &WavReader) -> LoudnessInfo {
        let ch = wav.spec.channels as usize;
        let frames = wav.frame_count();

        if frames == 0 {
            return LoudnessInfo {
                peak: 0.0,
                peak_db: f32::NEG_INFINITY,
                rms: 0.0,
                rms_db: f32::NEG_INFINITY,
                lufs: f32::NEG_INFINITY,
            };
        }

        let mut peak: f32 = 0.0;
        let mut sum_sq: f64 = 0.0;
        let mut count: u64 = 0;

        for frame in 0..frames {
            // Mix to mono
            let mut mono: f32 = 0.0;
            for c in 0..ch {
                let idx = frame * ch + c;
                if idx < wav.samples.len() {
                    mono += wav.samples[idx];
                }
            }
            mono /= ch as f32;

            let abs = mono.abs();
            if abs > peak {
                peak = abs;
            }
            sum_sq += (mono as f64) * (mono as f64);
            count += 1;
        }

        let rms = if count > 0 {
            (sum_sq / count as f64).sqrt() as f32
        } else {
            0.0
        };

        let peak_db = if peak > 0.0 {
            20.0 * peak.log10()
        } else {
            f32::NEG_INFINITY
        };

        let rms_db = if rms > 0.0 {
            20.0 * rms.log10()
        } else {
            f32::NEG_INFINITY
        };

        // Simplified LUFS ≈ RMS dB - 0.691 (K-weighting offset approximation) Real EBU R128 applies K-weighting filter + gating.
        let lufs = if rms > 0.0 {
            rms_db - 0.691
        } else {
            f32::NEG_INFINITY
        };

        LoudnessInfo {
            peak,
            peak_db,
            rms,
            rms_db,
            lufs,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::wav::WavSpec;

    #[test]
    fn silence_loudness() {
        let wav = WavReader {
            spec: WavSpec {
                channels: 1,
                sample_rate: 44100,
                bits_per_sample: 16,
            },
            samples: vec![0.0; 44100],
        };

        let info = LoudnessMeter::measure(&wav);
        assert_eq!(info.peak, 0.0);
        assert!(info.peak_db.is_infinite());
        assert!(info.rms_db.is_infinite());
    }

    #[test]
    fn full_scale_sine() {
        let sr = 44100usize;
        let samples: Vec<f32> = (0..sr)
            .map(|i| (2.0 * std::f32::consts::PI * 1000.0 * i as f32 / sr as f32).sin())
            .collect();

        let wav = WavReader {
            spec: WavSpec {
                channels: 1,
                sample_rate: sr as u32,
                bits_per_sample: 16,
            },
            samples,
        };

        let info = LoudnessMeter::measure(&wav);
        assert!(info.peak > 0.99, "Peak should be near 1.0");
        assert!(info.peak_db > -0.1, "Peak dB should be near 0 dBFS");
        // RMS of a sine wave = peak / √2 ≈ -3.01 dBFS
        assert!(
            (info.rms_db - (-3.01)).abs() < 0.5,
            "RMS dB should be ~-3 dBFS, got {}",
            info.rms_db
        );
    }

    #[test]
    fn empty_wav() {
        let wav = WavReader {
            spec: WavSpec {
                channels: 1,
                sample_rate: 44100,
                bits_per_sample: 16,
            },
            samples: Vec::new(),
        };

        let info = LoudnessMeter::measure(&wav);
        assert!(info.rms_db.is_infinite());
    }
}
