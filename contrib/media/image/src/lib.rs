//! Native image processing backend.
//!
//! Uses the `image` crate for fast image operations without
//! requiring FFmpeg to be installed.

#![warn(missing_docs)]

mod probe;
mod processor;

pub use probe::ImageProbe;
pub use processor::ImageProcessor;
