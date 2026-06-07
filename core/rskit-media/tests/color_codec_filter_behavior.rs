use rskit_media::codec::{
    Codec, CodecKind, CodecLevel, CodecProfile, audio, image, levels, subtitle, video,
};
use rskit_media::color::{ColorRange, ColorSpace, PixelFormat, pixel_formats};
use rskit_media::filter::{FilterTarget, ParamValue, filters};

fn assert_int(value: Option<&ParamValue>, expected: i64) {
    match value {
        Some(ParamValue::Int(actual)) => assert_eq!(*actual, expected),
        other => panic!("expected int {expected}, got {other:?}"),
    }
}

fn assert_float(value: Option<&ParamValue>, expected: f64) {
    match value {
        Some(ParamValue::Float(actual)) => assert!((*actual - expected).abs() < 0.000_001),
        other => panic!("expected float {expected}, got {other:?}"),
    }
}

fn assert_str(value: Option<&ParamValue>, expected: &str) {
    match value {
        Some(ParamValue::Str(actual)) => assert_eq!(actual, expected),
        other => panic!("expected string {expected}, got {other:?}"),
    }
}

#[test]
fn color_spaces_and_ranges_round_trip_known_ffmpeg_identifiers() {
    let spaces = [
        (ColorSpace::Bt601, Some("bt470bg")),
        (ColorSpace::Bt709, Some("bt709")),
        (ColorSpace::Bt2020, Some("bt2020nc")),
        (ColorSpace::Smpte240m, Some("smpte240m")),
        (ColorSpace::Srgb, Some("bt709")),
        (ColorSpace::DciP3, Some("bt2020nc")),
        (ColorSpace::DisplayP3, Some("bt2020nc")),
        (ColorSpace::Unknown, None),
    ];
    for (space, ffmpeg) in spaces {
        assert_eq!(space.as_ffmpeg_arg(), ffmpeg);
    }

    assert_eq!(ColorSpace::from_ffmpeg("bt709"), ColorSpace::Bt709);
    assert_eq!(ColorSpace::from_ffmpeg("smpte170m"), ColorSpace::Bt601);
    assert_eq!(ColorSpace::from_ffmpeg("bt2020c"), ColorSpace::Bt2020);
    assert_eq!(
        ColorSpace::from_ffmpeg("unknown-space"),
        ColorSpace::Unknown
    );

    assert_eq!(ColorRange::Limited.as_ffmpeg_arg(), "tv");
    assert_eq!(ColorRange::Full.as_ffmpeg_arg(), "pc");
    assert_eq!(ColorRange::from_ffmpeg("mpeg"), Some(ColorRange::Limited));
    assert_eq!(ColorRange::from_ffmpeg("jpeg"), Some(ColorRange::Full));
    assert_eq!(ColorRange::from_ffmpeg("studio"), None);
}

#[test]
fn pixel_formats_report_depth_alpha_and_display_identifiers() {
    let formats = [
        (pixel_formats::yuv420p(), "yuv420p", Some(8), false),
        (pixel_formats::yuv422p(), "yuv422p", Some(8), false),
        (pixel_formats::yuv444p(), "yuv444p", Some(8), false),
        (pixel_formats::yuv420p10le(), "yuv420p10le", Some(10), false),
        (pixel_formats::yuv420p12le(), "yuv420p12le", Some(12), false),
        (pixel_formats::rgb24(), "rgb24", Some(8), false),
        (pixel_formats::rgba(), "rgba", None, true),
        (pixel_formats::nv12(), "nv12", Some(8), false),
        (pixel_formats::p010le(), "p010le", Some(10), false),
        (PixelFormat::new("rgb48le"), "rgb48le", Some(16), false),
        (PixelFormat::new("yuva444p"), "yuva444p", None, true),
        (PixelFormat::new("vendor_fmt"), "vendor_fmt", None, false),
    ];

    for (format, id, bit_depth, has_alpha) in formats {
        assert_eq!(format.id(), id);
        assert_eq!(format.to_string(), id);
        assert_eq!(format.bit_depth(), bit_depth);
        assert_eq!(format.has_alpha(), has_alpha);
    }
}

#[test]
fn codecs_profiles_and_levels_preserve_known_and_custom_identifiers() {
    let codec_ids = [
        video::H264,
        video::H265,
        video::VP8,
        video::VP9,
        video::AV1,
        video::PRORES,
        video::MPEG2,
        video::MPEG4,
        video::THEORA,
        video::WMV3,
        audio::AAC,
        audio::OPUS,
        audio::MP3,
        audio::FLAC,
        audio::VORBIS,
        audio::PCM,
        audio::AC3,
        audio::EAC3,
        audio::WMA,
        audio::ALAC,
        image::PNG,
        image::JPEG,
        image::WEBP,
        image::GIF,
        image::BMP,
        image::TIFF,
        image::AVIF,
        image::HEIF,
        subtitle::SRT,
        subtitle::WEBVTT,
        subtitle::ASS,
        subtitle::SSA,
        subtitle::MOV_TEXT,
        "vendor_codec",
    ];
    for id in codec_ids {
        let codec = Codec::new(id);
        assert_eq!(codec.id(), id);
        assert_eq!(codec.to_string(), id);
    }

    let _kinds = [
        CodecKind::Video,
        CodecKind::Audio,
        CodecKind::Image,
        CodecKind::Subtitle,
        CodecKind::Unknown,
    ];

    let profiles = [
        (CodecProfile::H264Baseline, "baseline"),
        (CodecProfile::H264Main, "main"),
        (CodecProfile::H264High, "high"),
        (CodecProfile::H264High10, "high10"),
        (CodecProfile::H264High422, "high422"),
        (CodecProfile::H264High444, "high444p"),
        (CodecProfile::HevcMain, "main"),
        (CodecProfile::HevcMain10, "main10"),
        (CodecProfile::HevcMain12, "main12"),
        (CodecProfile::HevcMainStillPicture, "mainstillpicture"),
        (CodecProfile::Vp9Profile0, "0"),
        (CodecProfile::Vp9Profile1, "1"),
        (CodecProfile::Vp9Profile2, "2"),
        (CodecProfile::Vp9Profile3, "3"),
        (CodecProfile::Av1Main, "0"),
        (CodecProfile::Av1High, "1"),
        (CodecProfile::Av1Professional, "2"),
        (CodecProfile::AacLc, "aac_low"),
        (CodecProfile::AacHe, "aac_he"),
        (CodecProfile::AacHeV2, "aac_he_v2"),
        (CodecProfile::ProResProxy, "0"),
        (CodecProfile::ProResLt, "1"),
        (CodecProfile::ProRes422, "2"),
        (CodecProfile::ProResHq, "3"),
        (CodecProfile::ProRes4444, "4"),
        (
            CodecProfile::Other("vendor-profile".to_owned()),
            "vendor-profile",
        ),
    ];
    for (profile, ffmpeg_arg) in profiles {
        assert_eq!(profile.as_ffmpeg_arg(), ffmpeg_arg);
    }

    assert_eq!(
        CodecProfile::from_ffprobe("High 4:4:4 Predictive"),
        Some(CodecProfile::H264High444)
    );
    assert_eq!(
        CodecProfile::from_ffprobe("main still picture"),
        Some(CodecProfile::HevcMainStillPicture)
    );
    assert_eq!(
        CodecProfile::from_ffprobe("he-aacv2"),
        Some(CodecProfile::AacHeV2)
    );
    assert_eq!(
        CodecProfile::from_ffprobe("apch"),
        Some(CodecProfile::ProResHq)
    );
    assert_eq!(CodecProfile::from_ffprobe("unknown"), None);
    assert_eq!(
        CodecProfile::from_ffprobe("Studio Profile"),
        Some(CodecProfile::Other("Studio Profile".to_owned()))
    );

    let levels = [
        levels::h264_3_0(),
        levels::h264_3_1(),
        levels::h264_4_0(),
        levels::h264_4_1(),
        levels::h264_5_0(),
        levels::h264_5_1(),
        levels::h264_5_2(),
        CodecLevel::new("6.2"),
    ];
    for level in levels {
        assert_eq!(level.id(), level.to_string());
    }
}

#[test]
fn filters_preserve_targets_names_and_typed_parameters() {
    let denoise = filters::denoise(7);
    assert_eq!(denoise.name, "denoise");
    assert_eq!(denoise.target, FilterTarget::Video);
    assert_int(denoise.params.get("strength"), 7);

    let sharpen = filters::sharpen(1.5);
    assert_float(sharpen.params.get("amount"), 1.5);
    assert_float(filters::blur(2.0).params.get("radius"), 2.0);
    assert_float(filters::brightness(-0.25).params.get("value"), -0.25);
    assert_float(filters::contrast(1.25).params.get("value"), 1.25);
    assert_float(filters::saturation(0.75).params.get("value"), 0.75);
    assert_eq!(filters::grayscale().name, "grayscale");
    assert_eq!(filters::sepia().name, "sepia");
    assert_eq!(filters::stabilize().name, "stabilize");
    assert_eq!(filters::deinterlace().name, "deinterlace");
    assert_eq!(filters::custom_video("scale=320:240").name, "scale=320:240");

    assert_int(filters::high_pass(80).params.get("frequency"), 80);
    assert_int(filters::low_pass(12_000).params.get("frequency"), 12_000);
    let eq = filters::equalizer(1_000, 1.5, -3.0);
    assert_int(eq.params.get("frequency"), 1_000);
    assert_float(eq.params.get("width"), 1.5);
    assert_float(eq.params.get("gain"), -3.0);
    assert_float(filters::noise_reduction(0.4).params.get("amount"), 0.4);
    let compressor = filters::compressor(-12.0, 4.0);
    assert_float(compressor.params.get("threshold"), -12.0);
    assert_float(compressor.params.get("ratio"), 4.0);
    assert_eq!(
        filters::custom_audio("volume=0.5").target,
        FilterTarget::Audio
    );

    assert_float(filters::gamma(2.2).params.get("value"), 2.2);
    assert_float(filters::hue(180.0).params.get("degrees"), 180.0);
    assert_eq!(filters::invert().name, "invert");
    let fade = filters::fade(false, 1.0, 2.5);
    assert_str(fade.params.get("type"), "out");
    assert_float(fade.params.get("start"), 1.0);
    assert_float(fade.params.get("duration"), 2.5);
    let drawtext = filters::drawtext("caption", 24);
    assert_str(drawtext.params.get("text"), "caption");
    assert_int(drawtext.params.get("fontsize"), 24);
    let drawbox = filters::drawbox(1, 2, 3, 4, "red");
    assert_int(drawbox.params.get("x"), 1);
    assert_int(drawbox.params.get("y"), 2);
    assert_int(drawbox.params.get("w"), 3);
    assert_int(drawbox.params.get("h"), 4);
    assert_str(drawbox.params.get("color"), "red");
    let chromakey = filters::chromakey("green", 0.1, 0.2);
    assert_str(chromakey.params.get("color"), "green");
    assert_float(chromakey.params.get("similarity"), 0.1);
    assert_float(chromakey.params.get("blend"), 0.2);
    assert_str(
        filters::colorkey("blue", 0.3, 0.4).params.get("color"),
        "blue",
    );
    assert_float(filters::vignette(0.8).params.get("angle"), 0.8);
    let lens = filters::lenscorrection(-0.1, 0.05);
    assert_float(lens.params.get("k1"), -0.1);
    assert_float(lens.params.get("k2"), 0.05);
    assert_str(filters::lut3d("look.cube").params.get("file"), "look.cube");
    assert_eq!(filters::deshake().name, "deshake");
    assert_int(filters::fps(60).params.get("rate"), 60);
    assert_int(filters::minterpolate(120).params.get("fps"), 120);
    let balance = filters::colorbalance(-0.1, 0.0, 0.1);
    assert_float(balance.params.get("rs"), -0.1);
    assert_float(balance.params.get("gs"), 0.0);
    assert_float(balance.params.get("bs"), 0.1);
    assert_str(
        filters::curves("medium_contrast").params.get("preset"),
        "medium_contrast",
    );
    assert_eq!(filters::normalize().name, "normalize");
    assert_int(filters::deflicker(5).params.get("size"), 5);

    assert_float(filters::limiter(-1.0).params.get("limit"), -1.0);
    let gate = filters::gate(-40.0, 2.0);
    assert_float(gate.params.get("threshold"), -40.0);
    assert_float(gate.params.get("ratio"), 2.0);
    let loudnorm = filters::loudnorm(-16.0, -1.5, 11.0);
    assert_float(loudnorm.params.get("I"), -16.0);
    assert_float(loudnorm.params.get("TP"), -1.5);
    assert_float(loudnorm.params.get("LRA"), 11.0);
    let echo = filters::echo(0.8, 0.9, 100.0, 0.3);
    assert_float(echo.params.get("in_gain"), 0.8);
    assert_float(echo.params.get("out_gain"), 0.9);
    assert_float(echo.params.get("delays"), 100.0);
    assert_float(echo.params.get("decays"), 0.3);
    assert_int(filters::delay(250).params.get("ms"), 250);
    let silence = filters::silence_remove("-50dB", 0.25);
    assert_str(silence.params.get("threshold"), "-50dB");
    assert_float(silence.params.get("duration"), 0.25);
    assert_int(filters::aresample(48_000).params.get("rate"), 48_000);
    assert_float(filters::stereo_balance(-0.25).params.get("balance"), -0.25);
}
