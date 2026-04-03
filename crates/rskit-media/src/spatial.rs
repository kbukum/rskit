//! Spatial types for video and image processing.

use serde::{Deserialize, Serialize};

/// Width × Height in pixels.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct Resolution {
    /// Width in pixels.
    pub width: u32,
    /// Height in pixels.
    pub height: u32,
}

impl Resolution {
    /// Create a new resolution.
    pub fn new(w: u32, h: u32) -> Self {
        Self {
            width: w,
            height: h,
        }
    }

    /// 360p (640×360).
    pub fn p360() -> Self {
        Self::new(640, 360)
    }
    /// 480p (854×480).
    pub fn p480() -> Self {
        Self::new(854, 480)
    }
    /// 720p (1280×720).
    pub fn p720() -> Self {
        Self::new(1280, 720)
    }
    /// 1080p (1920×1080).
    pub fn p1080() -> Self {
        Self::new(1920, 1080)
    }
    /// 1440p (2560×1440).
    pub fn p1440() -> Self {
        Self::new(2560, 1440)
    }
    /// 4K (3840×2160).
    pub fn p4k() -> Self {
        Self::new(3840, 2160)
    }

    /// Aspect ratio as a simplified fraction (e.g., (16, 9)).
    pub fn aspect_ratio(&self) -> (u32, u32) {
        let g = gcd(self.width, self.height);
        if g == 0 {
            (0, 0)
        } else {
            (self.width / g, self.height / g)
        }
    }

    /// Aspect ratio as a floating-point value (width / height).
    pub fn aspect_ratio_f64(&self) -> f64 {
        if self.height == 0 {
            0.0
        } else {
            self.width as f64 / self.height as f64
        }
    }

    /// Whether the image is taller than it is wide.
    pub fn is_portrait(&self) -> bool {
        self.height > self.width
    }

    /// Whether the image is wider than it is tall.
    pub fn is_landscape(&self) -> bool {
        self.width > self.height
    }

    /// Whether width equals height.
    pub fn is_square(&self) -> bool {
        self.width == self.height
    }

    /// Total pixel count.
    pub fn pixel_count(&self) -> u64 {
        self.width as u64 * self.height as u64
    }

    /// Scale to fit within the given bounds, preserving aspect ratio.
    pub fn scale_to_fit(&self, max_width: u32, max_height: u32) -> Self {
        let w_ratio = max_width as f64 / self.width as f64;
        let h_ratio = max_height as f64 / self.height as f64;
        let ratio = w_ratio.min(h_ratio);
        Self::new(
            (self.width as f64 * ratio).round() as u32,
            (self.height as f64 * ratio).round() as u32,
        )
    }

    /// Scale to fill the given bounds, preserving aspect ratio (may overflow bounds).
    pub fn scale_to_fill(&self, width: u32, height: u32) -> Self {
        let w_ratio = width as f64 / self.width as f64;
        let h_ratio = height as f64 / self.height as f64;
        let ratio = w_ratio.max(h_ratio);
        Self::new(
            (self.width as f64 * ratio).round() as u32,
            (self.height as f64 * ratio).round() as u32,
        )
    }

    /// Scale by a factor (e.g., 0.5 for half size, 2.0 for double).
    pub fn scale_by(&self, factor: f64) -> Self {
        Self::new(
            (self.width as f64 * factor).round() as u32,
            (self.height as f64 * factor).round() as u32,
        )
    }
}

fn gcd(a: u32, b: u32) -> u32 {
    if b == 0 { a } else { gcd(b, a % b) }
}

/// Rational frame rate (numerator / denominator) for exact representation.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct FrameRate {
    /// Numerator.
    pub num: u32,
    /// Denominator.
    pub den: u32,
}

impl FrameRate {
    /// Create a new frame rate.
    pub fn new(num: u32, den: u32) -> Self {
        Self { num, den }
    }

    /// Create from an integer FPS value.
    pub fn fps(n: u32) -> Self {
        Self::new(n, 1)
    }

    /// 24 fps.
    pub fn fps_24() -> Self {
        Self::new(24, 1)
    }
    /// 25 fps.
    pub fn fps_25() -> Self {
        Self::new(25, 1)
    }
    /// 30 fps.
    pub fn fps_30() -> Self {
        Self::new(30, 1)
    }
    /// 50 fps.
    pub fn fps_50() -> Self {
        Self::new(50, 1)
    }
    /// 60 fps.
    pub fn fps_60() -> Self {
        Self::new(60, 1)
    }
    /// NTSC 29.97 fps.
    pub fn ntsc_30() -> Self {
        Self::new(30000, 1001)
    }
    /// NTSC 23.976 fps.
    pub fn ntsc_24() -> Self {
        Self::new(24000, 1001)
    }
    /// NTSC 59.94 fps.
    pub fn ntsc_60() -> Self {
        Self::new(60000, 1001)
    }

    /// Convert to floating-point FPS.
    pub fn as_f64(&self) -> f64 {
        if self.den == 0 {
            0.0
        } else {
            self.num as f64 / self.den as f64
        }
    }
}
