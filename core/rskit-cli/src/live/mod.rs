//! Multi-region live terminal rendering.
//!
//! [`LiveConsole`] stacks several concurrent output streams as fixed-height
//! tiles in a live area, flushing scrolled-out lines into scrollback. Its pure
//! per-stream viewport, [`RegionTail`], is reusable on its own for any
//! last-*k*-lines accounting over a byte stream.

mod console;
mod region;

pub use console::{LiveConfig, LiveConsole};
pub use region::RegionTail;
