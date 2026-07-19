//! Bounded per-region virtual terminal for the live console.
//!
//! A region hosts an arbitrary live child terminal,
//! so its output is applied to a small `rows × cols` cell grid — cursor addressing, erase, scroll,
//! and SGR all interpreted — rather than treated as appended lines.
//! This keeps a child's in-place redraws from corrupting the host terminal while still surfacing each scrolled-off row for a bounded failure replay.
//! [`RegionScreen`] is the public seam; the grid, cursor, SGR,
//! and parser mapping stay private to this module.

mod cursor;
mod grid;
mod perform;
mod region;
mod sgr;

pub use region::RegionScreen;
