//! Probe a media file and print its metadata.
//!
//! Usage:
//!   cargo run --bin probe -- path/to/video.mp4
//!
//! File paths are confined to the current working directory.

#[tokio::main]
async fn main() -> rskit::AppResult<()> {
    let args = media_demo::ProbeArgs::parse(std::env::args().skip(1));
    println!("{}", media_demo::run_probe(&args).await?);
    Ok(())
}
