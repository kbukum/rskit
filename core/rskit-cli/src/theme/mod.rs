//! Visual vocabulary shared by every renderer: color, status glyphs, and semantic styling.
//!
//! The theme layer resolves *how* output looks against the environment and user preference,
//! independent of *what* is being rendered:
//!
//! - [`color`] — a semantic [`Palette`] (success/error/warn/info/dim/bold) that honours [`NO_COLOR`]
//!   and TTY detection.
//! - [`glyph`] —
//!   a semantic [`Glyphs`] set (✓ ✗ ⚠ ℹ • → …) with a pure-ASCII fallback for terminals without UTF-8 support.
//! - `style` — a [`Theme`] built on a resolved [`Palette`] that renders bold headings and
//!   right-aligned, Cargo-like action lines keyed by a semantic [`Tone`].
//!
//! Each resolves from a single boolean (or the palette it wraps) so callers render identically
//! regardless of terminal capability, and each exposes an env-free constructor for deterministic tests.
//!
//! [`NO_COLOR`]: https://no-color.org

pub mod color;
pub mod glyph;
mod style;

pub use color::{
    ColorChoice, NO_COLOR_ENV, Palette, no_color_env_set, resolve_color, resolve_color_with,
};
pub use glyph::{Glyphs, UTF8_LOCALE_ENVS, unicode_env_enabled};
pub use style::{Theme, Tone};
