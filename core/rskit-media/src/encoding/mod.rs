//! Encoding vocabulary — codec, container format, color/pixel format, output
//! settings, presets, and the codec/format compatibility registry.

/// Codec identifiers, profiles, levels, and well-known constants.
pub mod codec;
/// Color space, color range, and pixel format types.
pub mod color;
/// Built-in codec/format catalog seeded into a fresh [`registry::Registry`].
mod defaults;
/// Container/file format identifiers and well-known constants.
pub mod format;
/// Output configuration, quality, and encoding settings.
pub mod output;
/// Preset output configurations for common formats.
pub mod presets;
/// Codec/format compatibility registry.
pub mod registry;
