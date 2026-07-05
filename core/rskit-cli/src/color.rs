//! Semantic terminal color with standard opt-out.
//!
//! A small, dependency-free color layer for CLI reporters: a [`ColorChoice`]
//! (`auto`/`always`/`never`) resolves — together with the [`NO_COLOR`] standard
//! and TTY detection — into an effective on/off [`Palette`]. The palette exposes
//! semantic styles (success/error/warn/info/dim/bold) that emit ANSI SGR escapes
//! when enabled and return the plain string when not, so the same rendering code
//! stays byte-clean on pipes and honors user preference.
//!
//! [`NO_COLOR`]: https://no-color.org

use std::borrow::Cow;
use std::io::IsTerminal;

/// The environment variable that, when present (regardless of value), disables
/// color regardless of any explicit choice ([the `NO_COLOR` standard](https://no-color.org)).
pub const NO_COLOR_ENV: &str = "NO_COLOR";

/// A user's requested color policy, before environment/TTY resolution.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
#[non_exhaustive]
pub enum ColorChoice {
    /// Enable color only when writing to a terminal and `NO_COLOR` is unset.
    #[default]
    Auto,
    /// Force color on (still overridden by `NO_COLOR`).
    Always,
    /// Force color off.
    Never,
}

impl ColorChoice {
    /// Parse a choice from its lowercase name (`auto`/`always`/`never`).
    ///
    /// Returns `None` for any other value so the caller can raise its own typed
    /// usage error naming the accepted values.
    #[must_use]
    pub fn from_name(name: &str) -> Option<Self> {
        match name {
            "auto" => Some(Self::Auto),
            "always" => Some(Self::Always),
            "never" => Some(Self::Never),
            _ => None,
        }
    }

    /// The canonical lowercase name of this choice.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Auto => "auto",
            Self::Always => "always",
            Self::Never => "never",
        }
    }
}

/// Resolve a [`ColorChoice`] into an effective on/off decision.
///
/// Resolution order (the `NO_COLOR` standard takes precedence over an explicit
/// request): if `NO_COLOR` is set → off; else `Always` → on, `Never` → off, and
/// `Auto` follows `is_terminal`. Reads the process environment for `NO_COLOR`;
/// use [`resolve_color_with`] for a fully injected, env-free decision.
#[must_use]
pub fn resolve_color(choice: ColorChoice, is_terminal: bool) -> bool {
    resolve_color_with(choice, no_color_env_set(), is_terminal)
}

/// Pure resolver core: fold an explicit `NO_COLOR` presence and TTY detection
/// into an effective on/off decision (env-free, so it is unit-testable).
///
/// `NO_COLOR` wins over every choice; otherwise `Always`/`Never` are absolute
/// and `Auto` follows `is_terminal`.
#[must_use]
pub const fn resolve_color_with(choice: ColorChoice, no_color: bool, is_terminal: bool) -> bool {
    if no_color {
        return false;
    }
    match choice {
        ColorChoice::Always => true,
        ColorChoice::Never => false,
        ColorChoice::Auto => is_terminal,
    }
}

/// Whether the `NO_COLOR` environment variable is present.
///
/// Per [the `NO_COLOR` standard](https://no-color.org) any presence disables
/// color, so an empty value (`NO_COLOR=`) still counts.
#[must_use]
pub fn no_color_env_set() -> bool {
    std::env::var_os(NO_COLOR_ENV).is_some()
}

/// A resolved, semantic color palette.
///
/// When disabled every style is the identity function, so callers render the
/// same way regardless of terminal capability. Construct it from a resolved
/// boolean via [`Palette::new`], or resolve a [`ColorChoice`] against a stream
/// with [`Palette::for_stream`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Palette {
    enabled: bool,
}

impl Palette {
    /// A palette with color explicitly enabled or disabled.
    #[must_use]
    pub const fn new(enabled: bool) -> Self {
        Self { enabled }
    }

    /// Resolve a palette for a specific output stream (e.g. `&std::io::stderr()`).
    ///
    /// Folds `NO_COLOR`, the `choice`, and the stream's TTY status via
    /// [`resolve_color`].
    #[must_use]
    pub fn for_stream(choice: ColorChoice, stream: &impl IsTerminal) -> Self {
        Self::new(resolve_color(choice, stream.is_terminal()))
    }

    /// Whether this palette emits color.
    #[must_use]
    pub const fn enabled(self) -> bool {
        self.enabled
    }

    /// Green — successful/complete status.
    #[must_use]
    pub fn success(self, text: &str) -> Cow<'_, str> {
        self.paint("32", text)
    }

    /// Red — failure/error status.
    #[must_use]
    pub fn error(self, text: &str) -> Cow<'_, str> {
        self.paint("31", text)
    }

    /// Yellow — warnings and attention.
    #[must_use]
    pub fn warn(self, text: &str) -> Cow<'_, str> {
        self.paint("33", text)
    }

    /// Cyan — informational/neutral highlight.
    #[must_use]
    pub fn info(self, text: &str) -> Cow<'_, str> {
        self.paint("36", text)
    }

    /// Dimmed — secondary detail (cache/skip).
    #[must_use]
    pub fn dim(self, text: &str) -> Cow<'_, str> {
        self.paint("2", text)
    }

    /// Bold — emphasis (headings, totals).
    #[must_use]
    pub fn bold(self, text: &str) -> Cow<'_, str> {
        self.paint("1", text)
    }

    /// Wrap `text` in an SGR code + reset when enabled, else borrow it verbatim.
    ///
    /// The disabled path returns a [`Cow::Borrowed`], so callers on pipes or
    /// redirects render without any allocation.
    fn paint<'a>(self, code: &str, text: &'a str) -> Cow<'a, str> {
        if self.enabled {
            Cow::Owned(format!("\u{1b}[{code}m{text}\u{1b}[0m"))
        } else {
            Cow::Borrowed(text)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{ColorChoice, Palette, resolve_color_with};
    use std::borrow::Cow;

    #[test]
    fn no_color_overrides_always() {
        assert!(!resolve_color_with(ColorChoice::Always, true, true));
        assert!(!resolve_color_with(ColorChoice::Always, true, false));
    }

    #[test]
    fn always_and_never_are_absolute_without_no_color() {
        assert!(resolve_color_with(ColorChoice::Always, false, false));
        assert!(!resolve_color_with(ColorChoice::Never, false, true));
    }

    #[test]
    fn auto_follows_terminal() {
        assert!(resolve_color_with(ColorChoice::Auto, false, true));
        assert!(!resolve_color_with(ColorChoice::Auto, false, false));
    }

    #[test]
    fn choice_round_trips_by_name() {
        for choice in [ColorChoice::Auto, ColorChoice::Always, ColorChoice::Never] {
            assert_eq!(ColorChoice::from_name(choice.as_str()), Some(choice));
        }
        assert_eq!(ColorChoice::from_name("bogus"), None);
    }

    #[test]
    fn disabled_palette_is_the_identity() {
        let plain = Palette::new(false);
        assert_eq!(plain.success("ok"), Cow::Borrowed("ok"));
        assert_eq!(plain.error("boom"), Cow::Borrowed("boom"));
        assert!(matches!(plain.success("ok"), Cow::Borrowed(_)));
        assert!(!plain.enabled());
    }

    #[test]
    fn enabled_palette_wraps_in_sgr_and_resets() {
        let color = Palette::new(true);
        assert_eq!(color.success("ok"), "\u{1b}[32mok\u{1b}[0m");
        assert_eq!(color.error("boom"), "\u{1b}[31mboom\u{1b}[0m");
        assert!(color.enabled());
    }
}
