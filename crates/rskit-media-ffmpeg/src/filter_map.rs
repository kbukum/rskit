//! Filter name → FFmpeg filter string mapping.

use std::collections::HashMap;

use rskit_media::filter::{Filter, FilterTarget, ParamValue, Params};

/// Convert a [`Filter`] into an FFmpeg filter string.
pub fn to_ffmpeg_filter(filter: &Filter) -> String {
    let map = FilterMap::default();
    map.resolve(filter)
}

type GeneratorFn = Box<dyn Fn(&Params) -> String + Send + Sync>;

struct FilterMap {
    video: HashMap<String, GeneratorFn>,
    audio: HashMap<String, GeneratorFn>,
}

impl Default for FilterMap {
    fn default() -> Self {
        let mut m = Self {
            video: HashMap::new(),
            audio: HashMap::new(),
        };
        m.load_defaults();
        m
    }
}

fn get_f64(params: &Params, key: &str) -> f64 {
    match params.get(key) {
        Some(ParamValue::Float(f)) => *f,
        Some(ParamValue::Int(i)) => *i as f64,
        _ => 0.0,
    }
}

fn get_i64(params: &Params, key: &str) -> i64 {
    match params.get(key) {
        Some(ParamValue::Int(i)) => *i,
        Some(ParamValue::Float(f)) => *f as i64,
        _ => 0,
    }
}

impl FilterMap {
    fn resolve(&self, filter: &Filter) -> String {
        let map = match filter.target {
            FilterTarget::Video => &self.video,
            FilterTarget::Audio => &self.audio,
        };

        match map.get(&filter.name) {
            Some(generator) => generator(&filter.params),
            // Unknown filters are passed through verbatim
            None => filter.name.clone(),
        }
    }

    fn load_defaults(&mut self) {
        // Video filters
        self.video.insert(
            "denoise".into(),
            Box::new(|p| format!("hqdn3d={}", get_i64(p, "strength"))),
        );
        self.video.insert(
            "sharpen".into(),
            Box::new(|p| format!("unsharp=5:5:{}", get_f64(p, "amount"))),
        );
        self.video.insert(
            "blur".into(),
            Box::new(|p| format!("boxblur={}", get_f64(p, "radius"))),
        );
        self.video.insert(
            "brightness".into(),
            Box::new(|p| format!("eq=brightness={}", get_f64(p, "value"))),
        );
        self.video.insert(
            "contrast".into(),
            Box::new(|p| format!("eq=contrast={}", get_f64(p, "value"))),
        );
        self.video.insert(
            "saturation".into(),
            Box::new(|p| format!("eq=saturation={}", get_f64(p, "value"))),
        );
        self.video.insert(
            "grayscale".into(),
            Box::new(|_| "format=gray".into()),
        );
        self.video.insert(
            "sepia".into(),
            Box::new(|_| {
                "colorchannelmixer=.393:.769:.189:0:.349:.686:.168:0:.272:.534:.131".into()
            }),
        );
        self.video.insert(
            "stabilize".into(),
            Box::new(|_| "vidstabdetect,vidstabtransform".into()),
        );
        self.video.insert(
            "deinterlace".into(),
            Box::new(|_| "yadif".into()),
        );

        // Audio filters
        self.audio.insert(
            "high_pass".into(),
            Box::new(|p| format!("highpass=f={}", get_i64(p, "frequency"))),
        );
        self.audio.insert(
            "low_pass".into(),
            Box::new(|p| format!("lowpass=f={}", get_i64(p, "frequency"))),
        );
        self.audio.insert(
            "equalizer".into(),
            Box::new(|p| {
                format!(
                    "equalizer=f={}:width_type=h:width={}:gain={}",
                    get_i64(p, "frequency"),
                    get_f64(p, "width"),
                    get_f64(p, "gain"),
                )
            }),
        );
        self.audio.insert(
            "noise_reduction".into(),
            Box::new(|p| {
                let amount = get_f64(p, "amount");
                format!("afftdn=nf=-{}", (amount * 25.0) as i64)
            }),
        );
        self.audio.insert(
            "compressor".into(),
            Box::new(|p| {
                format!(
                    "acompressor=threshold={}:ratio={}",
                    get_f64(p, "threshold"),
                    get_f64(p, "ratio"),
                )
            }),
        );
    }
}
