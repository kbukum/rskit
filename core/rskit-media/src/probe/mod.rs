//! Media probing traits and metadata types.
//!
//! Defines the [`MediaProbe`] trait for inspecting media files, along with
//! all associated types: [`MediaMetadata`], [`KeyframeInfo`],
//! [`SilenceInterval`], and [`Chapter`].

mod analysis;
mod media_probe;
mod metadata;

pub use analysis::{Chapter, KeyframeInfo, PictureType, SilenceInterval};
pub use media_probe::MediaProbe;
pub use metadata::MediaMetadata;
