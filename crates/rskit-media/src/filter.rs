//! Extensible filter types and convenience constructors.

use std::collections::HashMap;

use serde::{Deserialize, Serialize};

/// A named filter operation with typed parameters.
///
/// # Examples
///
/// ```rust
/// use rskit_media::filter::{self, filters};
///
/// let denoise = filters::denoise(3);
/// let sharpen = filters::sharpen(1.5);
/// let grayscale = filters::grayscale();
/// let custom = filters::custom_video("chromakey=0x00FF00:0.1:0.2");
/// ```
#[derive(Debug, Clone)]
pub struct Filter {
    /// Filter name (maps to backend filter name).
    pub name: String,
    /// Whether this filter targets video or audio.
    pub target: FilterTarget,
    /// Filter parameters.
    pub params: Params,
}

/// Which stream type a filter targets.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum FilterTarget {
    /// Video stream filter.
    Video,
    /// Audio stream filter.
    Audio,
}

/// Type-safe parameter map for filters.
#[derive(Debug, Clone, Default)]
pub struct Params(HashMap<String, ParamValue>);

impl Params {
    /// Create an empty parameter map.
    pub fn new() -> Self {
        Self(HashMap::new())
    }

    /// Set a parameter value (builder pattern).
    #[must_use]
    pub fn set(mut self, key: impl Into<String>, val: impl Into<ParamValue>) -> Self {
        self.0.insert(key.into(), val.into());
        self
    }

    /// Get a parameter value.
    pub fn get(&self, key: &str) -> Option<&ParamValue> {
        self.0.get(key)
    }

    /// Iterate over parameters.
    pub fn iter(&self) -> impl Iterator<Item = (&str, &ParamValue)> {
        self.0.iter().map(|(k, v)| (k.as_str(), v))
    }
}

/// A filter parameter value.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ParamValue {
    /// Integer value.
    Int(i64),
    /// Floating-point value.
    Float(f64),
    /// String value.
    Str(String),
    /// Boolean value.
    Bool(bool),
}

impl From<i64> for ParamValue {
    fn from(v: i64) -> Self { Self::Int(v) }
}

impl From<f64> for ParamValue {
    fn from(v: f64) -> Self { Self::Float(v) }
}

impl From<String> for ParamValue {
    fn from(v: String) -> Self { Self::Str(v) }
}

impl From<&str> for ParamValue {
    fn from(v: &str) -> Self { Self::Str(v.to_string()) }
}

impl From<bool> for ParamValue {
    fn from(v: bool) -> Self { Self::Bool(v) }
}

/// Convenience constructors for well-known filters.
pub mod filters {
    use super::*;

    // ── Video filters ────────────────────────────────────────────────

    /// Noise reduction with configurable strength (0–10).
    pub fn denoise(strength: u8) -> Filter {
        Filter {
            name: "denoise".into(),
            target: FilterTarget::Video,
            params: Params::new().set("strength", strength as i64),
        }
    }

    /// Sharpening with configurable amount.
    pub fn sharpen(amount: f32) -> Filter {
        Filter {
            name: "sharpen".into(),
            target: FilterTarget::Video,
            params: Params::new().set("amount", amount as f64),
        }
    }

    /// Blur with configurable radius.
    pub fn blur(radius: f32) -> Filter {
        Filter {
            name: "blur".into(),
            target: FilterTarget::Video,
            params: Params::new().set("radius", radius as f64),
        }
    }

    /// Brightness adjustment.
    pub fn brightness(value: f32) -> Filter {
        Filter {
            name: "brightness".into(),
            target: FilterTarget::Video,
            params: Params::new().set("value", value as f64),
        }
    }

    /// Contrast adjustment.
    pub fn contrast(value: f32) -> Filter {
        Filter {
            name: "contrast".into(),
            target: FilterTarget::Video,
            params: Params::new().set("value", value as f64),
        }
    }

    /// Saturation adjustment.
    pub fn saturation(value: f32) -> Filter {
        Filter {
            name: "saturation".into(),
            target: FilterTarget::Video,
            params: Params::new().set("value", value as f64),
        }
    }

    /// Convert to grayscale.
    pub fn grayscale() -> Filter {
        Filter {
            name: "grayscale".into(),
            target: FilterTarget::Video,
            params: Params::new(),
        }
    }

    /// Apply a sepia tone.
    pub fn sepia() -> Filter {
        Filter {
            name: "sepia".into(),
            target: FilterTarget::Video,
            params: Params::new(),
        }
    }

    /// Video stabilization.
    pub fn stabilize() -> Filter {
        Filter {
            name: "stabilize".into(),
            target: FilterTarget::Video,
            params: Params::new(),
        }
    }

    /// Deinterlacing.
    pub fn deinterlace() -> Filter {
        Filter {
            name: "deinterlace".into(),
            target: FilterTarget::Video,
            params: Params::new(),
        }
    }

    /// Pass a raw FFmpeg video filter string.
    pub fn custom_video(raw: impl Into<String>) -> Filter {
        Filter {
            name: raw.into(),
            target: FilterTarget::Video,
            params: Params::new(),
        }
    }

    // ── Audio filters ────────────────────────────────────────────────

    /// High-pass filter at given frequency (Hz).
    pub fn high_pass(freq_hz: u32) -> Filter {
        Filter {
            name: "high_pass".into(),
            target: FilterTarget::Audio,
            params: Params::new().set("frequency", freq_hz as i64),
        }
    }

    /// Low-pass filter at given frequency (Hz).
    pub fn low_pass(freq_hz: u32) -> Filter {
        Filter {
            name: "low_pass".into(),
            target: FilterTarget::Audio,
            params: Params::new().set("frequency", freq_hz as i64),
        }
    }

    /// Parametric equalizer band.
    pub fn equalizer(freq: u32, width: f32, gain: f32) -> Filter {
        Filter {
            name: "equalizer".into(),
            target: FilterTarget::Audio,
            params: Params::new()
                .set("frequency", freq as i64)
                .set("width", width as f64)
                .set("gain", gain as f64),
        }
    }

    /// Audio noise reduction.
    pub fn noise_reduction(amount: f32) -> Filter {
        Filter {
            name: "noise_reduction".into(),
            target: FilterTarget::Audio,
            params: Params::new().set("amount", amount as f64),
        }
    }

    /// Dynamic range compressor.
    pub fn compressor(threshold: f32, ratio: f32) -> Filter {
        Filter {
            name: "compressor".into(),
            target: FilterTarget::Audio,
            params: Params::new()
                .set("threshold", threshold as f64)
                .set("ratio", ratio as f64),
        }
    }

    /// Pass a raw FFmpeg audio filter string.
    pub fn custom_audio(raw: impl Into<String>) -> Filter {
        Filter {
            name: raw.into(),
            target: FilterTarget::Audio,
            params: Params::new(),
        }
    }
}
