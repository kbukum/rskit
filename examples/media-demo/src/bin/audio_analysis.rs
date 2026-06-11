//! Analyse a WAV file through the media registry.
//!
//! Usage:
//!   cargo run --bin audio_analysis -- path/to/audio.wav

#[tokio::main]
async fn main() -> rskit::AppResult<()> {
    let args = media_demo::AudioAnalysisArgs::parse(std::env::args().skip(1));
    println!("{}", media_demo::run_audio_analysis(&args).await?);
    Ok(())
}
