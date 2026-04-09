//! Analyse a WAV file — loudness, silence, waveform.
//!
//! Usage:
//!   cargo run --bin audio_analysis -- path/to/audio.wav

use rskit_media_audio::{
    LoudnessMeter, SilenceConfig, WavReader, WaveformConfig, detect_silence, generate_waveform,
};

fn main() -> rskit_errors::AppResult<()> {
    let path = std::env::args()
        .nth(1)
        .unwrap_or_else(|| "input.wav".into());

    let data = std::fs::read(&path).map_err(|e| {
        rskit_errors::AppError::new(rskit_errors::ErrorCode::NotFound, e.to_string())
    })?;

    let wav = WavReader::from_bytes(&data)?;

    println!("=== WAV Info: {path} ===");
    println!("Channels     : {}", wav.spec.channels);
    println!("Sample rate  : {} Hz", wav.spec.sample_rate);
    println!("Bits/sample  : {}", wav.spec.bits_per_sample);
    println!("Duration     : {:.2} s", wav.duration_secs());
    println!("Total frames : {}", wav.frame_count());

    // Loudness
    let loudness = LoudnessMeter::measure(&wav);
    println!("\n=== Loudness ===");
    println!("Peak    : {:.2} dBFS", loudness.peak_db);
    println!("RMS     : {:.2} dBFS", loudness.rms_db);
    println!("LUFS    : {:.2}", loudness.lufs);

    // Silence detection
    let silences = detect_silence(&wav, &SilenceConfig::default());
    println!("\n=== Silence regions ({}) ===", silences.len());
    for (i, s) in silences.iter().enumerate() {
        println!(
            "  {i}: {:.2}s – {:.2}s ({:.2}s)",
            s.start_secs,
            s.end_secs,
            s.duration_secs()
        );
    }

    // Waveform summary
    let waveform = generate_waveform(
        &wav,
        &WaveformConfig {
            bins: 20,
            channel: None,
        },
    );
    println!("\n=== Waveform (20 bins) ===");
    for (i, p) in waveform.iter().enumerate() {
        let bar_len = (p.rms * 50.0) as usize;
        let bar: String = "█".repeat(bar_len);
        println!("  {i:2}: peak={:.3} rms={:.3} {bar}", p.peak, p.rms);
    }

    Ok(())
}
