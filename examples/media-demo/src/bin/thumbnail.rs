//! Extract a thumbnail from a video at a given timestamp.
//!
//! Usage: cargo run --bin thumbnail -- input.mp4 thumb.jpg 5.0
//!
//! File paths are confined to the current working directory.

#[tokio::main]
async fn main() -> rskit::AppResult<()> {
    let args = media_demo::ThumbnailArgs::parse(std::env::args().skip(1));
    println!("{}", media_demo::run_thumbnail(&args).await?);
    Ok(())
}
