//! SVG chart rendering for benchmark results.
//!
//! Generates standalone SVG charts from benchmark curves and results.
//! No external dependencies — pure string building.

mod calibration;
mod comparison;
mod confusion;
mod distribution;
mod render;
mod roc;
mod svg;

pub use calibration::render_calibration;
pub use comparison::render_comparison;
pub use confusion::render_confusion;
pub use distribution::render_distribution;
pub use render::{RenderOption, render_all, with_size};
pub use roc::render_roc;
