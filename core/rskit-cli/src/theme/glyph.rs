//! Semantic status glyphs with a pure-ASCII fallback.
//!
//! A small companion to [`crate::theme::color`]: where the palette resolves *color*,
//! this resolves the small set of *symbols* CLIs use as status markers (a success check, an error cross, a bullet, an arrow…).
//! Unicode glyphs render on modern UTF-8 terminals; a legacy
//! or mis-encoded terminal falls back to a pure-ASCII stand-in
//! so output never turns into replacement characters.
//!
//! Like [`Palette`](crate::theme::Palette), a [`Glyphs`] value resolves from a single boolean,
//! so callers render identically regardless of capability,
//! and it exposes an env-free constructor ([`Glyphs::new`]) for deterministic tests alongside the environment-driven [`Glyphs::from_env`].

/// The locale environment variables consulted, in precedence order,
/// to decide whether the terminal encoding is UTF-8.
pub const UTF8_LOCALE_ENVS: [&str; 3] = ["LC_ALL", "LC_CTYPE", "LANG"];

/// Whether the process locale advertises a UTF-8 encoding.
///
/// Consults [`UTF8_LOCALE_ENVS`] in order
/// and reports `true` when any is set to a value naming UTF-8 (case-insensitive, `utf-8` or `utf8`).
/// When none is set the result is `false`, so the ASCII fallback is the safe default.
#[must_use]
pub fn unicode_env_enabled() -> bool {
    UTF8_LOCALE_ENVS.iter().any(|key| {
        std::env::var(key).is_ok_and(|value| {
            let value = value.to_ascii_lowercase();
            value.contains("utf-8") || value.contains("utf8")
        })
    })
}

/// A resolved set of semantic status glyphs.
///
/// When Unicode is disabled every accessor returns its ASCII fallback,
/// so the same rendering code stays byte-clean on terminals that cannot display the Unicode symbols.
/// Construct it from a resolved boolean via [`Glyphs::new`],
/// or from the process locale via [`Glyphs::from_env`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Glyphs {
    unicode: bool,
}

impl Glyphs {
    /// A glyph set with Unicode explicitly enabled or disabled.
    #[must_use]
    pub const fn new(unicode: bool) -> Self {
        Self { unicode }
    }

    /// Resolve a glyph set from the process locale via [`unicode_env_enabled`].
    #[must_use]
    pub fn from_env() -> Self {
        Self::new(unicode_env_enabled())
    }

    /// Whether this set emits Unicode glyphs.
    #[must_use]
    pub const fn unicode(self) -> bool {
        self.unicode
    }

    /// Success / completed — `✓` (ASCII `v`).
    #[must_use]
    pub const fn success(self) -> &'static str {
        self.pick("✓", "v")
    }

    /// Failure / error — `✗` (ASCII `x`).
    #[must_use]
    pub const fn error(self) -> &'static str {
        self.pick("✗", "x")
    }

    /// Warning / attention — `⚠` (ASCII `!`).
    #[must_use]
    pub const fn warning(self) -> &'static str {
        self.pick("⚠", "!")
    }

    /// Informational — `ℹ` (ASCII `i`).
    #[must_use]
    pub const fn info(self) -> &'static str {
        self.pick("ℹ", "i")
    }

    /// List bullet — `•` (ASCII `*`).
    #[must_use]
    pub const fn bullet(self) -> &'static str {
        self.pick("•", "*")
    }

    /// Progression arrow — `→` (ASCII `->`).
    #[must_use]
    pub const fn arrow(self) -> &'static str {
        self.pick("→", "->")
    }

    /// Selection pointer — `❯` (ASCII `>`).
    #[must_use]
    pub const fn pointer(self) -> &'static str {
        self.pick("❯", ">")
    }

    /// Inline answer/input marker — `»` (ASCII `>`).
    #[must_use]
    pub const fn answer(self) -> &'static str {
        self.pick("»", ">")
    }

    /// Selected radio option — `◉` (ASCII `(*)`).
    #[must_use]
    pub const fn radio_on(self) -> &'static str {
        self.pick("◉", "(*)")
    }

    /// Unselected radio option — `○` (ASCII `( )`).
    #[must_use]
    pub const fn radio_off(self) -> &'static str {
        self.pick("○", "( )")
    }

    /// Upward navigation arrow — `↑` (ASCII `^`).
    #[must_use]
    pub const fn arrow_up(self) -> &'static str {
        self.pick("↑", "^")
    }

    /// Downward navigation arrow — `↓` (ASCII `v`).
    #[must_use]
    pub const fn arrow_down(self) -> &'static str {
        self.pick("↓", "v")
    }

    /// Truncation ellipsis — `…` (ASCII `...`).
    #[must_use]
    pub const fn ellipsis(self) -> &'static str {
        self.pick("…", "...")
    }

    /// Choose the Unicode glyph when enabled, else the ASCII fallback.
    const fn pick(self, unicode: &'static str, ascii: &'static str) -> &'static str {
        if self.unicode { unicode } else { ascii }
    }
}

#[cfg(test)]
mod tests {
    use super::{Glyphs, unicode_env_enabled};

    #[test]
    fn env_probe_and_from_env_agree_and_are_callable() {
        // `unicode_env_enabled` reads the process locale; `from_env` must mirror it.
        // Both are exercised here without mutating the (forbidden-unsafe) environment,
        // so the result tracks whatever locale the runner exposes.
        assert_eq!(Glyphs::from_env().unicode(), unicode_env_enabled());
    }

    #[test]
    fn unicode_set_emits_symbols() {
        let glyphs = Glyphs::new(true);
        assert!(glyphs.unicode());
        assert_eq!(glyphs.success(), "✓");
        assert_eq!(glyphs.error(), "✗");
        assert_eq!(glyphs.warning(), "⚠");
        assert_eq!(glyphs.info(), "ℹ");
        assert_eq!(glyphs.bullet(), "•");
        assert_eq!(glyphs.arrow(), "→");
        assert_eq!(glyphs.pointer(), "❯");
        assert_eq!(glyphs.answer(), "»");
        assert_eq!(glyphs.radio_on(), "◉");
        assert_eq!(glyphs.radio_off(), "○");
        assert_eq!(glyphs.arrow_up(), "↑");
        assert_eq!(glyphs.arrow_down(), "↓");
        assert_eq!(glyphs.ellipsis(), "…");
    }

    #[test]
    fn ascii_fallback_is_byte_clean() {
        let glyphs = Glyphs::new(false);
        assert!(!glyphs.unicode());
        for symbol in [
            glyphs.success(),
            glyphs.error(),
            glyphs.warning(),
            glyphs.info(),
            glyphs.bullet(),
            glyphs.arrow(),
            glyphs.pointer(),
            glyphs.answer(),
            glyphs.radio_on(),
            glyphs.radio_off(),
            glyphs.arrow_up(),
            glyphs.arrow_down(),
            glyphs.ellipsis(),
        ] {
            assert!(symbol.is_ascii(), "fallback must be ASCII: {symbol:?}");
        }
    }
}
