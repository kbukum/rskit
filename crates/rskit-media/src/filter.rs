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
#[derive(Debug, Clone, Serialize, Deserialize)]
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
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
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
    fn from(v: i64) -> Self {
        Self::Int(v)
    }
}

impl From<f64> for ParamValue {
    fn from(v: f64) -> Self {
        Self::Float(v)
    }
}

impl From<String> for ParamValue {
    fn from(v: String) -> Self {
        Self::Str(v)
    }
}

impl From<&str> for ParamValue {
    fn from(v: &str) -> Self {
        Self::Str(v.to_string())
    }
}

impl From<bool> for ParamValue {
    fn from(v: bool) -> Self {
        Self::Bool(v)
    }
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

    // ── New video filters ────────────────────────────────────────────

    /// Adjust gamma.
    pub fn gamma(value: f32) -> Filter {
        Filter {
            name: "gamma".into(),
            target: FilterTarget::Video,
            params: Params::new().set("value", value as f64),
        }
    }

    /// Adjust hue by rotation angle (degrees).
    pub fn hue(degrees: f32) -> Filter {
        Filter {
            name: "hue".into(),
            target: FilterTarget::Video,
            params: Params::new().set("degrees", degrees as f64),
        }
    }

    /// Invert/negate colors.
    pub fn invert() -> Filter {
        Filter {
            name: "invert".into(),
            target: FilterTarget::Video,
            params: Params::new(),
        }
    }

    /// Video fade (in or out).
    pub fn fade(fade_in: bool, start_secs: f32, duration_secs: f32) -> Filter {
        Filter {
            name: "fade".into(),
            target: FilterTarget::Video,
            params: Params::new()
                .set("type", if fade_in { "in" } else { "out" })
                .set("start", start_secs as f64)
                .set("duration", duration_secs as f64),
        }
    }

    /// Draw text overlay on video.
    pub fn drawtext(text: impl Into<String>, fontsize: u32) -> Filter {
        Filter {
            name: "drawtext".into(),
            target: FilterTarget::Video,
            params: Params::new()
                .set("text", text.into())
                .set("fontsize", fontsize as i64),
        }
    }

    /// Draw a box on video.
    pub fn drawbox(x: u32, y: u32, w: u32, h: u32, color: impl Into<String>) -> Filter {
        Filter {
            name: "drawbox".into(),
            target: FilterTarget::Video,
            params: Params::new()
                .set("x", x as i64)
                .set("y", y as i64)
                .set("w", w as i64)
                .set("h", h as i64)
                .set("color", color.into()),
        }
    }

    /// Chroma-key (green screen removal).
    pub fn chromakey(color: impl Into<String>, similarity: f32, blend: f32) -> Filter {
        Filter {
            name: "chromakey".into(),
            target: FilterTarget::Video,
            params: Params::new()
                .set("color", color.into())
                .set("similarity", similarity as f64)
                .set("blend", blend as f64),
        }
    }

    /// Color-key removal.
    pub fn colorkey(color: impl Into<String>, similarity: f32, blend: f32) -> Filter {
        Filter {
            name: "colorkey".into(),
            target: FilterTarget::Video,
            params: Params::new()
                .set("color", color.into())
                .set("similarity", similarity as f64)
                .set("blend", blend as f64),
        }
    }

    /// Apply a vignette effect.
    pub fn vignette(angle: f32) -> Filter {
        Filter {
            name: "vignette".into(),
            target: FilterTarget::Video,
            params: Params::new().set("angle", angle as f64),
        }
    }

    /// Lens distortion correction.
    pub fn lenscorrection(k1: f64, k2: f64) -> Filter {
        Filter {
            name: "lenscorrection".into(),
            target: FilterTarget::Video,
            params: Params::new().set("k1", k1).set("k2", k2),
        }
    }

    /// Apply a 3D LUT file.
    pub fn lut3d(file: impl Into<String>) -> Filter {
        Filter {
            name: "lut3d".into(),
            target: FilterTarget::Video,
            params: Params::new().set("file", file.into()),
        }
    }

    /// Deshake (simple stabilization).
    pub fn deshake() -> Filter {
        Filter {
            name: "deshake".into(),
            target: FilterTarget::Video,
            params: Params::new(),
        }
    }

    /// Change output frame rate.
    pub fn fps(rate: u32) -> Filter {
        Filter {
            name: "fps".into(),
            target: FilterTarget::Video,
            params: Params::new().set("rate", rate as i64),
        }
    }

    /// Motion-interpolated frame rate conversion.
    pub fn minterpolate(target_fps: u32) -> Filter {
        Filter {
            name: "minterpolate".into(),
            target: FilterTarget::Video,
            params: Params::new().set("fps", target_fps as i64),
        }
    }

    /// Color balance adjustment.
    pub fn colorbalance(rs: f64, gs: f64, bs: f64) -> Filter {
        Filter {
            name: "colorbalance".into(),
            target: FilterTarget::Video,
            params: Params::new().set("rs", rs).set("gs", gs).set("bs", bs),
        }
    }

    /// Color curves preset.
    pub fn curves(preset: impl Into<String>) -> Filter {
        Filter {
            name: "curves".into(),
            target: FilterTarget::Video,
            params: Params::new().set("preset", preset.into()),
        }
    }

    /// Normalize video levels.
    pub fn normalize() -> Filter {
        Filter {
            name: "normalize".into(),
            target: FilterTarget::Video,
            params: Params::new(),
        }
    }

    /// Deflicker.
    pub fn deflicker(size: u32) -> Filter {
        Filter {
            name: "deflicker".into(),
            target: FilterTarget::Video,
            params: Params::new().set("size", size as i64),
        }
    }

    // ── New audio filters ────────────────────────────────────────────

    /// Audio limiter.
    pub fn limiter(limit_db: f32) -> Filter {
        Filter {
            name: "limiter".into(),
            target: FilterTarget::Audio,
            params: Params::new().set("limit", limit_db as f64),
        }
    }

    /// Audio noise gate.
    pub fn gate(threshold_db: f32, ratio: f32) -> Filter {
        Filter {
            name: "gate".into(),
            target: FilterTarget::Audio,
            params: Params::new()
                .set("threshold", threshold_db as f64)
                .set("ratio", ratio as f64),
        }
    }

    /// EBU R128 loudness normalization with custom targets.
    pub fn loudnorm(integrated: f64, true_peak: f64, lra: f64) -> Filter {
        Filter {
            name: "loudnorm".into(),
            target: FilterTarget::Audio,
            params: Params::new()
                .set("I", integrated)
                .set("TP", true_peak)
                .set("LRA", lra),
        }
    }

    /// Echo effect.
    pub fn echo(in_gain: f64, out_gain: f64, delays_ms: f64, decays: f64) -> Filter {
        Filter {
            name: "echo".into(),
            target: FilterTarget::Audio,
            params: Params::new()
                .set("in_gain", in_gain)
                .set("out_gain", out_gain)
                .set("delays", delays_ms)
                .set("decays", decays),
        }
    }

    /// Audio delay (milliseconds).
    pub fn delay(ms: u32) -> Filter {
        Filter {
            name: "delay".into(),
            target: FilterTarget::Audio,
            params: Params::new().set("ms", ms as i64),
        }
    }

    /// Remove silence from audio.
    pub fn silence_remove(threshold_db: impl Into<String>, min_duration: f64) -> Filter {
        Filter {
            name: "silenceremove".into(),
            target: FilterTarget::Audio,
            params: Params::new()
                .set("threshold", threshold_db.into())
                .set("duration", min_duration),
        }
    }

    /// Resample audio to a different sample rate.
    pub fn aresample(rate: u32) -> Filter {
        Filter {
            name: "aresample".into(),
            target: FilterTarget::Audio,
            params: Params::new().set("rate", rate as i64),
        }
    }

    /// Stereo balance adjustment (-1.0 left to 1.0 right).
    pub fn stereo_balance(balance: f64) -> Filter {
        Filter {
            name: "stereotools".into(),
            target: FilterTarget::Audio,
            params: Params::new().set("balance", balance),
        }
    }
}
