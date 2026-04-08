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

fn get_str<'a>(params: &'a Params, key: &str) -> &'a str {
    match params.get(key) {
        Some(ParamValue::Str(s)) => s.as_str(),
        _ => "",
    }
}

fn get_str_or<'a>(params: &'a Params, key: &str, default: &'a str) -> &'a str {
    match params.get(key) {
        Some(ParamValue::Str(s)) if !s.is_empty() => s.as_str(),
        _ => default,
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
        // ── Video filters ────────────────────────────────────────────

        // Noise / cleanup
        self.video.insert(
            "denoise".into(),
            Box::new(|p| format!("hqdn3d={}", get_i64(p, "strength"))),
        );
        self.video
            .insert("deinterlace".into(), Box::new(|_| "yadif".into()));
        self.video.insert(
            "stabilize".into(),
            Box::new(|_| "vidstabdetect,vidstabtransform".into()),
        );
        self.video.insert(
            "deshake".into(),
            Box::new(|_| "deshake".into()),
        );
        self.video.insert(
            "removegrain".into(),
            Box::new(|p| format!("removegrain=m={}", get_i64(p, "mode"))),
        );
        self.video.insert(
            "deflicker".into(),
            Box::new(|p| format!("deflicker=s={}", get_i64(p, "size").max(1))),
        );

        // Sharpness / blur
        self.video.insert(
            "sharpen".into(),
            Box::new(|p| format!("unsharp=5:5:{}", get_f64(p, "amount"))),
        );
        self.video.insert(
            "blur".into(),
            Box::new(|p| format!("boxblur={}", get_f64(p, "radius"))),
        );

        // Color / tone
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
            "gamma".into(),
            Box::new(|p| format!("eq=gamma={}", get_f64(p, "value"))),
        );
        self.video.insert(
            "hue".into(),
            Box::new(|p| format!("hue=h={}", get_f64(p, "degrees"))),
        );
        self.video
            .insert("grayscale".into(), Box::new(|_| "format=gray".into()));
        self.video.insert(
            "sepia".into(),
            Box::new(|_| {
                "colorchannelmixer=.393:.769:.189:0:.349:.686:.168:0:.272:.534:.131".into()
            }),
        );
        self.video.insert(
            "invert".into(),
            Box::new(|_| "negate".into()),
        );
        self.video.insert(
            "colorbalance".into(),
            Box::new(|p| {
                format!(
                    "colorbalance=rs={}:gs={}:bs={}",
                    get_f64(p, "rs"),
                    get_f64(p, "gs"),
                    get_f64(p, "bs"),
                )
            }),
        );
        self.video.insert(
            "curves".into(),
            Box::new(|p| {
                let preset = get_str(p, "preset");
                if preset.is_empty() {
                    "curves".into()
                } else {
                    format!("curves=preset={preset}")
                }
            }),
        );
        self.video.insert(
            "normalize".into(),
            Box::new(|_| "normalize".into()),
        );
        self.video.insert(
            "lut3d".into(),
            Box::new(|p| format!("lut3d='{}'", get_str(p, "file"))),
        );

        // Keying
        self.video.insert(
            "chromakey".into(),
            Box::new(|p| {
                format!(
                    "chromakey={}:{}:{}",
                    get_str(p, "color"),
                    get_f64(p, "similarity"),
                    get_f64(p, "blend"),
                )
            }),
        );
        self.video.insert(
            "colorkey".into(),
            Box::new(|p| {
                format!(
                    "colorkey={}:{}:{}",
                    get_str(p, "color"),
                    get_f64(p, "similarity"),
                    get_f64(p, "blend"),
                )
            }),
        );

        // Geometry / effects
        self.video.insert(
            "vignette".into(),
            Box::new(|p| {
                let angle = get_f64(p, "angle");
                if angle > 0.0 {
                    format!("vignette=a={angle}")
                } else {
                    "vignette".into()
                }
            }),
        );
        self.video.insert(
            "lenscorrection".into(),
            Box::new(|p| {
                format!(
                    "lenscorrection=k1={}:k2={}",
                    get_f64(p, "k1"),
                    get_f64(p, "k2"),
                )
            }),
        );
        self.video.insert(
            "perspective".into(),
            Box::new(|p| {
                format!(
                    "perspective=x0={}:y0={}:x1={}:y1={}:x2={}:y2={}:x3={}:y3={}",
                    get_str(p, "x0"), get_str(p, "y0"),
                    get_str(p, "x1"), get_str(p, "y1"),
                    get_str(p, "x2"), get_str(p, "y2"),
                    get_str(p, "x3"), get_str(p, "y3"),
                )
            }),
        );

        // Overlay / text / drawing
        self.video.insert(
            "drawtext".into(),
            Box::new(|p| {
                let text = get_str(p, "text");
                let x = get_str_or(p, "x", "(w-text_w)/2");
                let y = get_str_or(p, "y", "(h-text_h)/2");
                let fontsize = get_i64(p, "fontsize");
                let fontcolor = get_str_or(p, "fontcolor", "white");
                format!("drawtext=text='{text}':x={x}:y={y}:fontsize={fontsize}:fontcolor={fontcolor}")
            }),
        );
        self.video.insert(
            "drawbox".into(),
            Box::new(|p| {
                format!(
                    "drawbox=x={}:y={}:w={}:h={}:color={}:t={}",
                    get_i64(p, "x"),
                    get_i64(p, "y"),
                    get_i64(p, "w"),
                    get_i64(p, "h"),
                    get_str_or(p, "color", "red"),
                    get_str_or(p, "thickness", "3"),
                )
            }),
        );

        // Fading
        self.video.insert(
            "fade".into(),
            Box::new(|p| {
                let fade_type = get_str_or(p, "type", "in");
                let start = get_f64(p, "start");
                let duration = get_f64(p, "duration");
                format!("fade=t={fade_type}:st={start}:d={duration}")
            }),
        );

        // Frame rate / timing
        self.video.insert(
            "fps".into(),
            Box::new(|p| format!("fps={}", get_i64(p, "rate"))),
        );
        self.video.insert(
            "minterpolate".into(),
            Box::new(|p| {
                let fps = get_i64(p, "fps");
                let mi_mode = get_str_or(p, "mi_mode", "mci");
                format!("minterpolate=fps={fps}:mi_mode={mi_mode}")
            }),
        );

        // Tiling / splitting
        self.video.insert(
            "tile".into(),
            Box::new(|p| {
                format!("tile={}x{}", get_i64(p, "cols"), get_i64(p, "rows"))
            }),
        );

        // ── Audio filters ────────────────────────────────────────────

        // EQ / frequency
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

        // Dynamics
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
        self.audio.insert(
            "limiter".into(),
            Box::new(|p| format!("alimiter=limit={}", get_f64(p, "limit"))),
        );
        self.audio.insert(
            "gate".into(),
            Box::new(|p| {
                format!(
                    "agate=threshold={}:ratio={}",
                    get_f64(p, "threshold"),
                    get_f64(p, "ratio"),
                )
            }),
        );

        // Noise
        self.audio.insert(
            "noise_reduction".into(),
            Box::new(|p| {
                let amount = get_f64(p, "amount");
                format!("afftdn=nf=-{}", (amount * 25.0) as i64)
            }),
        );
        self.audio.insert(
            "silenceremove".into(),
            Box::new(|p| {
                let threshold = get_str_or(p, "threshold", "-50dB");
                let duration = get_f64(p, "duration");
                format!("silenceremove=start_periods=1:start_silence={duration}:start_threshold={threshold}")
            }),
        );
        self.audio.insert(
            "silencedetect".into(),
            Box::new(|p| {
                let noise = get_str_or(p, "noise", "-50dB");
                let duration = get_f64(p, "duration");
                format!("silencedetect=noise={noise}:d={duration}")
            }),
        );

        // Loudness
        self.audio.insert(
            "loudnorm".into(),
            Box::new(|p| {
                let i = get_f64(p, "I");
                let tp = get_f64(p, "TP");
                let lra = get_f64(p, "LRA");
                if i != 0.0 || tp != 0.0 || lra != 0.0 {
                    format!("loudnorm=I={}:TP={}:LRA={}", 
                        if i != 0.0 { i } else { -24.0 },
                        if tp != 0.0 { tp } else { -2.0 },
                        if lra != 0.0 { lra } else { 7.0 },
                    )
                } else {
                    "loudnorm".into()
                }
            }),
        );

        // Effects
        self.audio.insert(
            "echo".into(),
            Box::new(|p| {
                format!(
                    "aecho={}:{}:{}:{}",
                    get_f64(p, "in_gain"),
                    get_f64(p, "out_gain"),
                    get_f64(p, "delays"),
                    get_f64(p, "decays"),
                )
            }),
        );
        self.audio.insert(
            "delay".into(),
            Box::new(|p| format!("adelay={}|{}", get_i64(p, "ms"), get_i64(p, "ms"))),
        );

        // Format / channel
        self.audio.insert(
            "aformat".into(),
            Box::new(|p| {
                format!(
                    "aformat=sample_rates={}:channel_layouts={}",
                    get_str(p, "sample_rate"),
                    get_str(p, "channel_layout"),
                )
            }),
        );
        self.audio.insert(
            "aresample".into(),
            Box::new(|p| format!("aresample={}", get_i64(p, "rate"))),
        );
        self.audio.insert(
            "channelmap".into(),
            Box::new(|p| format!("channelmap=map={}", get_str(p, "map"))),
        );

        // Stereo tools
        self.audio.insert(
            "stereotools".into(),
            Box::new(|p| {
                format!(
                    "stereotools=balance_out={}",
                    get_f64(p, "balance"),
                )
            }),
        );

        // Analysis (detect-only filters)
        self.audio.insert(
            "volumedetect".into(),
            Box::new(|_| "volumedetect".into()),
        );
        self.audio.insert(
            "astats".into(),
            Box::new(|_| "astats".into()),
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rskit_media::filter::{Filter, FilterTarget, ParamValue, Params};

    fn video_filter(name: &str, params: Params) -> Filter {
        Filter {
            name: name.into(),
            target: FilterTarget::Video,
            params,
        }
    }

    fn audio_filter(name: &str, params: Params) -> Filter {
        Filter {
            name: name.into(),
            target: FilterTarget::Audio,
            params,
        }
    }

    #[test]
    fn golden_denoise() {
        let f = video_filter("denoise", Params::new().set("strength", ParamValue::Int(5)));
        assert_eq!(to_ffmpeg_filter(&f), "hqdn3d=5");
    }

    #[test]
    fn golden_sharpen() {
        let f = video_filter(
            "sharpen",
            Params::new().set("amount", ParamValue::Float(1.5)),
        );
        assert_eq!(to_ffmpeg_filter(&f), "unsharp=5:5:1.5");
    }

    #[test]
    fn golden_blur() {
        let f = video_filter("blur", Params::new().set("radius", ParamValue::Float(3.0)));
        assert_eq!(to_ffmpeg_filter(&f), "boxblur=3");
    }

    #[test]
    fn golden_brightness() {
        let f = video_filter(
            "brightness",
            Params::new().set("value", ParamValue::Float(0.2)),
        );
        assert_eq!(to_ffmpeg_filter(&f), "eq=brightness=0.2");
    }

    #[test]
    fn golden_grayscale() {
        let f = video_filter("grayscale", Params::new());
        assert_eq!(to_ffmpeg_filter(&f), "format=gray");
    }

    #[test]
    fn golden_sepia() {
        let f = video_filter("sepia", Params::new());
        let result = to_ffmpeg_filter(&f);
        assert!(result.starts_with("colorchannelmixer="), "got: {result}");
    }

    #[test]
    fn golden_deinterlace() {
        let f = video_filter("deinterlace", Params::new());
        assert_eq!(to_ffmpeg_filter(&f), "yadif");
    }

    #[test]
    fn golden_high_pass() {
        let f = audio_filter(
            "high_pass",
            Params::new().set("frequency", ParamValue::Int(200)),
        );
        assert_eq!(to_ffmpeg_filter(&f), "highpass=f=200");
    }

    #[test]
    fn golden_low_pass() {
        let f = audio_filter(
            "low_pass",
            Params::new().set("frequency", ParamValue::Int(3000)),
        );
        assert_eq!(to_ffmpeg_filter(&f), "lowpass=f=3000");
    }

    #[test]
    fn golden_compressor() {
        let f = audio_filter(
            "compressor",
            Params::new()
                .set("threshold", ParamValue::Float(-20.0))
                .set("ratio", ParamValue::Float(4.0)),
        );
        assert_eq!(to_ffmpeg_filter(&f), "acompressor=threshold=-20:ratio=4");
    }

    #[test]
    fn golden_unknown_filter_passthrough() {
        let f = video_filter("custom_unknown", Params::new());
        assert_eq!(to_ffmpeg_filter(&f), "custom_unknown");
    }
}
