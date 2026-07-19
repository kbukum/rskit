//! Transcode a video using a pipeline.
//!
//! Usage: cargo run --bin transcode -- input.mp4 output.mp4
//!
//! File paths are confined to the current working directory.

#[tokio::main]
async fn main() -> rskit::AppResult<()> {
    let args = media_demo::TranscodeArgs::parse(std::env::args().skip(1));
    println!("{}", media_demo::run_transcode(&args).await?);
    Ok(())
}
