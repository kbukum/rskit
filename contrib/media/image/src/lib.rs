//! Native image processing backend.
//!
//! Uses the `image` crate for fast image operations without requiring FFmpeg to be installed.

#![warn(missing_docs)]

mod config;
mod io;
mod probe;
mod processor;
mod registry;

pub use config::Config;
pub use registry::register;
