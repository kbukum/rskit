//! Media operation types for pipeline building.

mod compose;
/// Configuration for `ApplyFilter` operations.
pub mod filter_config;
/// Configuration for `Interpolate` operations.
pub mod interpolate;
mod operation;
/// Configuration for `AddOverlay` operations.
pub mod overlay_config;
/// Configuration for `DetectScenes` operations.
pub mod scene_detect;
mod spatial;
/// Configuration for `AddSubtitles` operations.
pub mod subtitle_config;
/// Configuration for `GenerateThumbnail` operations.
pub mod thumbnail;
/// Configuration for `Upscale` operations.
pub mod upscale;

pub use compose::*;
pub use filter_config::*;
pub use interpolate::*;
pub use operation::MediaOp;
pub use overlay_config::*;
pub use scene_detect::*;
pub use spatial::*;
pub use subtitle_config::*;
pub use thumbnail::*;
pub use upscale::*;
