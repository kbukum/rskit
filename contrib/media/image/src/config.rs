//! Image backend configuration and decode resource limits.

/// Default compressed source byte limit for native image operations.
pub const DEFAULT_MAX_SOURCE_BYTES: u64 = 64 * 1024 * 1024;
/// Default decoded pixel limit for native image operations.
pub const DEFAULT_MAX_PIXELS: u64 = 100_000_000;
/// Default maximum decoded-byte to compressed-byte ratio.
pub const DEFAULT_MAX_DECODE_RATIO: u64 = 256;

/// Configuration for the native image media backend.
///
/// These limits are applied before or immediately after decoding so untrusted
/// images cannot force unbounded reads or decompression-bomb style allocations.
#[derive(Debug, Clone)]
pub struct Config {
    /// Maximum compressed image source bytes read into memory before decoding.
    pub max_source_bytes: u64,
    /// Maximum `width * height` decoded pixels accepted for a single image.
    pub max_pixels: u64,
    /// Maximum decoded-byte to compressed-byte ratio accepted after decoding.
    ///
    /// Values below `1` are treated as `1` internally.
    pub max_decode_ratio: u64,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            max_source_bytes: DEFAULT_MAX_SOURCE_BYTES,
            max_pixels: DEFAULT_MAX_PIXELS,
            max_decode_ratio: DEFAULT_MAX_DECODE_RATIO,
        }
    }
}

impl Config {
    /// Override the compressed source byte limit.
    #[must_use]
    pub fn with_max_source_bytes(mut self, max_source_bytes: u64) -> Self {
        self.max_source_bytes = max_source_bytes;
        self
    }

    /// Override the decoded pixel limit.
    #[must_use]
    pub fn with_max_pixels(mut self, max_pixels: u64) -> Self {
        self.max_pixels = max_pixels;
        self
    }

    /// Override the decoded-byte to compressed-byte ratio limit.
    #[must_use]
    pub fn with_max_decode_ratio(mut self, max_decode_ratio: u64) -> Self {
        self.max_decode_ratio = max_decode_ratio;
        self
    }

    pub(crate) fn image_limits(&self) -> image::Limits {
        let mut limits = image::Limits::default();
        limits.max_image_width = Some(self.max_dimension());
        limits.max_image_height = Some(self.max_dimension());
        limits.max_alloc = Some(self.max_pixels.saturating_mul(16));
        limits
    }

    fn max_dimension(&self) -> u32 {
        u32::try_from(self.max_pixels.min(u64::from(u32::MAX)))
            .unwrap_or(u32::MAX)
            .max(1)
    }
}
