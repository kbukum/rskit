//! Golden/snapshot tests for rskit-media-audio using real WAV fixtures.

use std::path::PathBuf;

use rskit_media_audio::{
    LoudnessMeter, WavReader,
    detect_silence, generate_waveform,
    SilenceConfig, WaveformConfig,
};

fn fixtures_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .parent()
        .unwrap()
        .join("tests/fixtures")
}

// ── Test 1: WAV reader on real fixture ──────────────────────────────────────

#[test]
fn golden_wav_reader_ai_generated() {
    let data = std::fs::read(fixtures_dir().join("audio/ai-generated.wav"))
        .expect("read fixture");
    let wav = WavReader::from_bytes(&data).expect("parse WAV");

    insta::assert_json_snapshot!("wav_reader_ai_generated", {
        ".duration_secs" => insta::rounded_redaction(2),
    }, serde_json::json!({
        "channels": wav.spec.channels,
        "sample_rate": wav.spec.sample_rate,
        "bits_per_sample": wav.spec.bits_per_sample,
        "frame_count": wav.frame_count(),
        "duration_secs": wav.duration_secs(),
    }));
}

#[test]
fn golden_wav_reader_real_voice() {
    let data = std::fs::read(fixtures_dir().join("audio/real-voice.wav"))
        .expect("read fixture");
    let wav = WavReader::from_bytes(&data).expect("parse WAV");

    insta::assert_json_snapshot!("wav_reader_real_voice", {
        ".duration_secs" => insta::rounded_redaction(2),
    }, serde_json::json!({
        "channels": wav.spec.channels,
        "sample_rate": wav.spec.sample_rate,
        "bits_per_sample": wav.spec.bits_per_sample,
        "frame_count": wav.frame_count(),
        "duration_secs": wav.duration_secs(),
    }));
}

// ── Test 2: Loudness on real fixture ────────────────────────────────────────

#[test]
fn golden_loudness_ai_generated() {
    let data = std::fs::read(fixtures_dir().join("audio/ai-generated.wav"))
        .expect("read fixture");
    let wav = WavReader::from_bytes(&data).expect("parse WAV");
    let info = LoudnessMeter::measure(&wav);

    insta::assert_json_snapshot!("loudness_ai_generated", {
        ".peak" => insta::rounded_redaction(3),
        ".peak_db" => insta::rounded_redaction(1),
        ".rms" => insta::rounded_redaction(3),
        ".rms_db" => insta::rounded_redaction(1),
        ".lufs" => insta::rounded_redaction(1),
    }, serde_json::json!({
        "peak": info.peak,
        "peak_db": info.peak_db,
        "rms": info.rms,
        "rms_db": info.rms_db,
        "lufs": info.lufs,
        "has_audio": info.peak > 0.0,
    }));
}

#[test]
fn golden_loudness_real_voice() {
    let data = std::fs::read(fixtures_dir().join("audio/real-voice.wav"))
        .expect("read fixture");
    let wav = WavReader::from_bytes(&data).expect("parse WAV");
    let info = LoudnessMeter::measure(&wav);

    insta::assert_json_snapshot!("loudness_real_voice", {
        ".peak" => insta::rounded_redaction(3),
        ".peak_db" => insta::rounded_redaction(1),
        ".rms" => insta::rounded_redaction(3),
        ".rms_db" => insta::rounded_redaction(1),
        ".lufs" => insta::rounded_redaction(1),
    }, serde_json::json!({
        "peak": info.peak,
        "peak_db": info.peak_db,
        "rms": info.rms,
        "rms_db": info.rms_db,
        "lufs": info.lufs,
        "has_audio": info.peak > 0.0,
    }));
}

// ── Test 3: Silence detection on real fixture ───────────────────────────────

#[test]
fn golden_silence_detection_real_voice() {
    let data = std::fs::read(fixtures_dir().join("audio/real-voice.wav"))
        .expect("read fixture");
    let wav = WavReader::from_bytes(&data).expect("parse WAV");

    let regions = detect_silence(&wav, &SilenceConfig {
        threshold: 0.01,
        min_duration_secs: 0.05,
    });

    let region_summaries: Vec<serde_json::Value> = regions
        .iter()
        .map(|r| serde_json::json!({
            "start": (r.start_secs * 100.0).round() / 100.0,
            "end": (r.end_secs * 100.0).round() / 100.0,
            "duration": (r.duration_secs() * 100.0).round() / 100.0,
        }))
        .collect();

    insta::assert_json_snapshot!("silence_detection_real_voice", serde_json::json!({
        "total_duration_secs": wav.duration_secs(),
        "silence_region_count": regions.len(),
        "regions": region_summaries,
    }));
}

// ── Test 4: Waveform on real fixture ────────────────────────────────────────

#[test]
fn golden_waveform_ai_generated() {
    let data = std::fs::read(fixtures_dir().join("audio/ai-generated.wav"))
        .expect("read fixture");
    let wav = WavReader::from_bytes(&data).expect("parse WAV");

    let points = generate_waveform(&wav, &WaveformConfig {
        bins: 10,
        channel: None,
    });

    let summary: Vec<serde_json::Value> = points
        .iter()
        .enumerate()
        .map(|(i, p)| serde_json::json!({
            "bin": i,
            "peak": (p.peak * 1000.0).round() / 1000.0,
            "rms": (p.rms * 1000.0).round() / 1000.0,
        }))
        .collect();

    insta::assert_json_snapshot!("waveform_ai_generated", serde_json::json!({
        "bin_count": points.len(),
        "bins": summary,
    }));
}

#[test]
fn golden_waveform_real_voice() {
    let data = std::fs::read(fixtures_dir().join("audio/real-voice.wav"))
        .expect("read fixture");
    let wav = WavReader::from_bytes(&data).expect("parse WAV");

    let points = generate_waveform(&wav, &WaveformConfig {
        bins: 10,
        channel: None,
    });

    let summary: Vec<serde_json::Value> = points
        .iter()
        .enumerate()
        .map(|(i, p)| serde_json::json!({
            "bin": i,
            "peak": (p.peak * 1000.0).round() / 1000.0,
            "rms": (p.rms * 1000.0).round() / 1000.0,
        }))
        .collect();

    insta::assert_json_snapshot!("waveform_real_voice", serde_json::json!({
        "bin_count": points.len(),
        "bins": summary,
    }));
}
