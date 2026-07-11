//! Multi-region live terminal rendering.
//!
//! [`LiveConsole`] stacks several concurrent output streams as fixed-height
//! tiles in a live area, flushing scrolled-out rows into scrollback. Each tile
//! is backed by a [`RegionScreen`] — a bounded virtual terminal — so a child
//! that redraws in place renders faithfully without corrupting the host.

mod renderer;
mod screen;

pub use renderer::{LiveConfig, LiveConsole};
pub use screen::RegionScreen;
