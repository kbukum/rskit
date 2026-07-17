//! Pure Rust audio backend configuration.

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
