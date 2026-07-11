//! Multi-region live terminal rendering.
//!
//! [`LiveConsole`] stacks several concurrent output streams as fixed-height
//! tiles in a live area. The tiles are an ephemeral peek: scrolled-out rows are
//! dropped from the live view and retained only for a bounded failure replay, so
//! scrollback carries durable signal (verdicts and failure blocks) rather than a
//! flood of transient progress. Each tile is backed by a [`RegionScreen`] — a
//! bounded virtual terminal — so a child that redraws in place renders
//! faithfully without corrupting the host.

mod renderer;
mod screen;

pub use renderer::{LiveConfig, LiveConsole};
pub use screen::RegionScreen;
