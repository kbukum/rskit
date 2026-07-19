//! Visual vocabulary shared by every renderer: color and status glyphs.
//!
//! The theme layer resolves *how* output looks against the environment and user preference,
//! independent of *what* is being rendered:
//!
//! - [`color`] — a semantic [`Palette`] (success/error/warn/info/dim/bold) that honours [`NO_COLOR`]
//!   and TTY detection.
//! - [`glyph`] —
//!   a semantic [`Glyphs`] set (✓ ✗ ⚠ ℹ • → …) with a pure-ASCII fallback for terminals without UTF-8 support.
//!
//! Both resolve from a single boolean so callers render identically regardless of terminal capability,
//! and both expose an env-free constructor for deterministic tests.
//!
//! [`NO_COLOR`]: https://no-color.org

pub mod color;
pub mod glyph;

pub use color::{
    ColorChoice, NO_COLOR_ENV, Palette, no_color_env_set, resolve_color, resolve_color_with,
};
pub use glyph::{Glyphs, UTF8_LOCALE_ENVS, unicode_env_enabled};
