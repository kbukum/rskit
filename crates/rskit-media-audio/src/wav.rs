//! Minimal WAV file reader (PCM only, no compression).
//!
//! Supports mono and stereo, 8/16/24/32-bit integer and 32-bit float samples.

use rskit_errors::{AppError, AppResult, ErrorCode};

/// WAV file specification.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct WavSpec {
    /// Number of channels (1 = mono, 2 = stereo).
    pub channels: u16,
    /// Sample rate in Hz (e.g. 44100, 48000).
    pub sample_rate: u32,
    /// Bits per sample (8, 16, 24, 32).
    pub bits_per_sample: u16,
}

/// A decoded WAV file with samples normalised to `f32` in `[-1.0, 1.0]`.
#[derive(Debug, Clone)]
pub struct WavReader {
    /// Audio specification.
    pub spec: WavSpec,
    /// Interleaved samples normalised to `[-1.0, 1.0]`.
    pub samples: Vec<f32>,
}

impl WavReader {
    /// Parse a WAV file from raw bytes.
    pub fn from_bytes(data: &[u8]) -> AppResult<Self> {
        if data.len() < 44 {
            return Err(AppError::new(ErrorCode::InvalidInput, "WAV file too small"));
        }

        // RIFF header
        if &data[0..4] != b"RIFF" || &data[8..12] != b"WAVE" {
            return Err(AppError::new(
                ErrorCode::InvalidInput,
                "Not a valid WAV file (missing RIFF/WAVE header)",
            ));
        }

        // Find fmt chunk
        let (spec, fmt_end) = Self::parse_fmt_chunk(data)?;

        // Find data chunk
        let (data_offset, data_len) = Self::find_chunk(data, b"data", fmt_end)?;

        let samples = Self::decode_samples(&data[data_offset..data_offset + data_len], &spec)?;

        Ok(Self { spec, samples })
    }

    /// Total duration in seconds.
    #[must_use]
    pub fn duration_secs(&self) -> f64 {
        let total_frames = self.samples.len() / self.spec.channels as usize;
        total_frames as f64 / self.spec.sample_rate as f64
    }

    /// Number of frames (samples per channel).
    #[must_use]
    pub fn frame_count(&self) -> usize {
        self.samples.len() / self.spec.channels as usize
    }

    /// Get samples for a specific channel (0-indexed).
    #[must_use]
    pub fn channel_samples(&self, channel: usize) -> Vec<f32> {
        let ch = self.spec.channels as usize;
        if channel >= ch {
            return Vec::new();
        }
        self.samples
            .iter()
            .skip(channel)
            .step_by(ch)
            .copied()
            .collect()
    }

    fn parse_fmt_chunk(data: &[u8]) -> AppResult<(WavSpec, usize)> {
        let (offset, chunk_len) = Self::find_chunk(data, b"fmt ", 12)?;

        if chunk_len < 16 {
            return Err(AppError::new(
                ErrorCode::InvalidInput,
                "WAV fmt chunk too small",
            ));
        }

        let audio_format = u16::from_le_bytes([data[offset], data[offset + 1]]);
        // 1 = PCM integer, 3 = IEEE float
        if audio_format != 1 && audio_format != 3 {
            return Err(AppError::new(
                ErrorCode::InvalidInput,
                format!("Unsupported WAV audio format: {audio_format} (only PCM/float supported)"),
            ));
        }

        let channels = u16::from_le_bytes([data[offset + 2], data[offset + 3]]);
        let sample_rate = u32::from_le_bytes([
            data[offset + 4],
            data[offset + 5],
            data[offset + 6],
            data[offset + 7],
        ]);
        let bits_per_sample = u16::from_le_bytes([data[offset + 14], data[offset + 15]]);

        Ok((
            WavSpec {
                channels,
                sample_rate,
                bits_per_sample,
            },
            offset + chunk_len,
        ))
    }

    fn find_chunk(data: &[u8], id: &[u8; 4], start: usize) -> AppResult<(usize, usize)> {
        let mut pos = start;
        while pos + 8 <= data.len() {
            if &data[pos..pos + 4] == id {
                let size = u32::from_le_bytes([
                    data[pos + 4],
                    data[pos + 5],
                    data[pos + 6],
                    data[pos + 7],
                ]) as usize;
                let data_start = pos + 8;
                let available = data.len().saturating_sub(data_start);
                return Ok((data_start, size.min(available)));
            }
            let chunk_size = u32::from_le_bytes([
                data[pos + 4],
                data[pos + 5],
                data[pos + 6],
                data[pos + 7],
            ]) as usize;
            // Chunks are word-aligned
            pos += 8 + ((chunk_size + 1) & !1);
        }
        Err(AppError::new(
            ErrorCode::InvalidInput,
            format!(
                "WAV chunk '{}' not found",
                std::str::from_utf8(id).unwrap_or("????")
            ),
        ))
    }

    fn decode_samples(data: &[u8], spec: &WavSpec) -> AppResult<Vec<f32>> {
        let bps = spec.bits_per_sample;
        let bytes_per_sample = (bps / 8) as usize;
        if bytes_per_sample == 0 {
            return Err(AppError::new(
                ErrorCode::InvalidInput,
                "Invalid bits_per_sample",
            ));
        }
        let sample_count = data.len() / bytes_per_sample;
        let mut samples = Vec::with_capacity(sample_count);

        for i in 0..sample_count {
            let offset = i * bytes_per_sample;
            let sample = match bps {
                8 => {
                    // 8-bit WAV is unsigned
                    (data[offset] as f32 - 128.0) / 128.0
                }
                16 => {
                    let val = i16::from_le_bytes([data[offset], data[offset + 1]]);
                    val as f32 / i16::MAX as f32
                }
                24 => {
                    let val = i32::from_le_bytes([0, data[offset], data[offset + 1], data[offset + 2]]);
                    // Sign-extend from 24-bit
                    let val = if val & 0x0080_0000 != 0 {
                        val | (0xFF << 24) as i32
                    } else {
                        val
                    };
                    val as f32 / 8_388_607.0
                }
                32 => {
                    // Could be int or float — assume float if data looks like it
                    f32::from_le_bytes([
                        data[offset],
                        data[offset + 1],
                        data[offset + 2],
                        data[offset + 3],
                    ])
                }
                _ => {
                    return Err(AppError::new(
                        ErrorCode::InvalidInput,
                        format!("Unsupported bits_per_sample: {bps}"),
                    ));
                }
            };
            samples.push(sample.clamp(-1.0, 1.0));
        }

        Ok(samples)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_wav_16bit_mono(sample_rate: u32, samples: &[i16]) -> Vec<u8> {
        let data_size = (samples.len() * 2) as u32;
        let file_size = 36 + data_size;
        let mut buf = Vec::with_capacity(file_size as usize + 8);

        // RIFF header
        buf.extend_from_slice(b"RIFF");
        buf.extend_from_slice(&file_size.to_le_bytes());
        buf.extend_from_slice(b"WAVE");

        // fmt chunk
        buf.extend_from_slice(b"fmt ");
        buf.extend_from_slice(&16u32.to_le_bytes()); // chunk size
        buf.extend_from_slice(&1u16.to_le_bytes()); // PCM
        buf.extend_from_slice(&1u16.to_le_bytes()); // mono
        buf.extend_from_slice(&sample_rate.to_le_bytes());
        buf.extend_from_slice(&(sample_rate * 2).to_le_bytes()); // byte rate
        buf.extend_from_slice(&2u16.to_le_bytes()); // block align
        buf.extend_from_slice(&16u16.to_le_bytes()); // bits per sample

        // data chunk
        buf.extend_from_slice(b"data");
        buf.extend_from_slice(&data_size.to_le_bytes());
        for &s in samples {
            buf.extend_from_slice(&s.to_le_bytes());
        }

        buf
    }

    #[test]
    fn parse_valid_wav() {
        let samples = vec![0, 16383, -16384, 32767, -32768];
        let wav_data = make_wav_16bit_mono(44100, &samples);
        let reader = WavReader::from_bytes(&wav_data).unwrap();
        assert_eq!(reader.spec.channels, 1);
        assert_eq!(reader.spec.sample_rate, 44100);
        assert_eq!(reader.spec.bits_per_sample, 16);
        assert_eq!(reader.frame_count(), 5);
    }

    #[test]
    fn duration_calculation() {
        let samples = vec![0i16; 44100]; // 1 second at 44100 Hz
        let wav_data = make_wav_16bit_mono(44100, &samples);
        let reader = WavReader::from_bytes(&wav_data).unwrap();
        assert!((reader.duration_secs() - 1.0).abs() < 0.001);
    }

    #[test]
    fn rejects_non_wav() {
        let result = WavReader::from_bytes(b"not a wav file at all!!!!!!!!!!!!!!!!!!!!!!!!!!");
        assert!(result.is_err());
    }

    #[test]
    fn rejects_too_small() {
        let result = WavReader::from_bytes(b"tiny");
        assert!(result.is_err());
    }
}
