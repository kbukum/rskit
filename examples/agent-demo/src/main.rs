//! Agent Demo — Interactive media processing agent built with rskit.
//!
//! Demonstrates the public `rskit` facade modules: `worker` background tasks,
//! `cli` output helpers, `storage` I/O, and `media` + `media_image` processing.
//!
//! Run: cargo run -p agent-demo

use rskit::AppResult;

#[tokio::main]
async fn main() -> AppResult<()> {
    let cancel = tokio_util::sync::CancellationToken::new();
    let cancel_clone = cancel.clone();

    // Handle Ctrl+C
    tokio::spawn(async move {
        tokio::signal::ctrl_c().await.ok();
        cancel_clone.cancel();
    });

    agent_demo::shell::run(cancel).await
}
