//! Audio waveform generation — peak and RMS values for visualisation.

use crate::wav::WavReader;

/// Configuration for waveform generation.
#[derive(Debug, Clone)]
pub struct WaveformConfig {
    /// Number of output points (bins). Each bin summarises a range of samples.
    pub bins: usize,
    /// Channel to analyse (0 = first/left, 1 = right). `None` mixes all channels.
    pub channel: Option<usize>,
}

impl Default for WaveformConfig {
    fn default() -> Self {
        Self {
            bins: 800,
            channel: None,
        }
    }
}

/// A single point in the waveform.
#[derive(Debug, Clone, Copy)]
pub struct WaveformPoint {
    /// Peak amplitude in this bin (`0.0..=1.0`).
    pub peak: f32,
    /// RMS amplitude in this bin (`0.0..=1.0`).
    pub rms: f32,
    /// Minimum sample in this bin (`-1.0..=1.0`).
    pub min: f32,
    /// Maximum sample in this bin (`-1.0..=1.0`).
    pub max: f32,
}

/// Generate a waveform from a decoded WAV.
///
/// Returns one [`WaveformPoint`] per bin. If the WAV has fewer samples than
/// bins the output is truncated.
pub fn generate_waveform(wav: &WavReader, config: &WaveformConfig) -> Vec<WaveformPoint> {
    let samples: Vec<f32> = match config.channel {
        Some(ch) => wav.channel_samples(ch),
        None => {
            // Mix all channels to mono
            let ch = wav.spec.channels as usize;
            if ch <= 1 {
                wav.samples.clone()
            } else {
                let frames = wav.frame_count();
                (0..frames)
                    .map(|f| {
                        let sum: f32 = (0..ch).map(|c| wav.samples[f * ch + c]).sum();
                        sum / ch as f32
                    })
                    .collect()
            }
        }
    };

    if samples.is_empty() || config.bins == 0 {
        return Vec::new();
    }

    let bins = config.bins.min(samples.len());
    let samples_per_bin = samples.len() as f64 / bins as f64;
    let mut points = Vec::with_capacity(bins);

    for i in 0..bins {
        let start = (i as f64 * samples_per_bin) as usize;
        let end = ((i + 1) as f64 * samples_per_bin) as usize;
        let end = end.min(samples.len());
        let slice = &samples[start..end];

        if slice.is_empty() {
            points.push(WaveformPoint {
                peak: 0.0,
                rms: 0.0,
                min: 0.0,
                max: 0.0,
            });
            continue;
        }

        let mut min_val = f32::MAX;
        let mut max_val = f32::MIN;
        let mut sum_sq: f64 = 0.0;
        let mut peak: f32 = 0.0;

        for &s in slice {
            let abs = s.abs();
            if abs > peak {
                peak = abs;
            }
            if s < min_val {
                min_val = s;
            }
            if s > max_val {
                max_val = s;
            }
            sum_sq += (s as f64) * (s as f64);
        }

        let rms = (sum_sq / slice.len() as f64).sqrt() as f32;

        points.push(WaveformPoint {
            peak,
            rms,
            min: min_val,
            max: max_val,
        });
    }

    points
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::wav::WavSpec;

    #[test]
    fn waveform_sine_wave() {
        // 1000 Hz sine wave at 44100 Hz for 1 second
        let sample_rate = 44100u32;
        let num_samples = sample_rate as usize;
        let samples: Vec<f32> = (0..num_samples)
            .map(|i| (2.0 * std::f32::consts::PI * 1000.0 * i as f32 / sample_rate as f32).sin())
            .collect();

        let wav = WavReader {
            spec: WavSpec {
                channels: 1,
                sample_rate,
                bits_per_sample: 16,
            },
            samples,
        };

        let config = WaveformConfig {
            bins: 100,
            channel: None,
        };

        let points = generate_waveform(&wav, &config);
        assert_eq!(points.len(), 100);

        // All bins should have non-zero RMS for a sine wave
        for p in &points {
            assert!(p.rms > 0.0, "RMS should be positive for sine wave");
            assert!(p.peak > 0.0);
            assert!(p.peak <= 1.0);
        }
    }

    #[test]
    fn waveform_silence() {
        let wav = WavReader {
            spec: WavSpec {
                channels: 1,
                sample_rate: 44100,
                bits_per_sample: 16,
            },
            samples: vec![0.0; 4410],
        };

        let points = generate_waveform(&wav, &WaveformConfig::default());
        for p in &points {
            assert_eq!(p.peak, 0.0);
            assert_eq!(p.rms, 0.0);
        }
    }
}
