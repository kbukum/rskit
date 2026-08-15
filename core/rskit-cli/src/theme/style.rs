//! Semantic styling for headings and Cargo-like action lines.

use crate::theme::Palette;

/// Semantic outcome applied to an action label.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum Tone {
    /// Successful or completed work.
    Success,
    /// Failed work.
    Error,
    /// Work requiring attention.
    Warning,
    /// Neutral progress or information.
    Info,
    /// Secondary or skipped work.
    Dim,
}

/// Reusable terminal theme built on a resolved [`Palette`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Theme {
    palette: Palette,
    action_width: usize,
}

impl Theme {
    /// Default width of the right-aligned action-label column.
    pub const DEFAULT_ACTION_WIDTH: usize = 12;

    /// Create a theme from a resolved palette.
    #[must_use]
    pub const fn new(palette: Palette) -> Self {
        Self {
            palette,
            action_width: Self::DEFAULT_ACTION_WIDTH,
        }
    }

    /// Set the right-aligned action-label width.
    #[must_use]
    pub const fn with_action_width(mut self, width: usize) -> Self {
        self.action_width = width;
        self
    }

    /// Return the resolved palette.
    #[must_use]
    pub const fn palette(self) -> Palette {
        self.palette
    }

    /// Render a bold heading.
    #[must_use]
    pub fn heading(self, title: &str) -> String {
        self.palette.bold(title).into_owned()
    }

    /// Render a right-aligned, bold semantic action label followed by unstyled detail.
    #[must_use]
    pub fn action(self, label: &str, detail: &str, tone: Tone) -> String {
        let aligned = format!("{label:>width$}", width = self.action_width);
        let colored = match tone {
            Tone::Success => self.palette.success(&aligned),
            Tone::Error => self.palette.error(&aligned),
            Tone::Warning => self.palette.warn(&aligned),
            Tone::Info => self.palette.info(&aligned),
            Tone::Dim => self.palette.dim(&aligned),
        };
        format!("{} {detail}", self.palette.bold(&colored))
    }
}

#[cfg(test)]
mod tests {
    use super::{Theme, Tone};
    use crate::theme::Palette;

    #[test]
    fn disabled_palette_keeps_actions_aligned_and_escape_free() {
        let output = Theme::new(Palette::new(false)).action("Done", "crate", Tone::Success);
        assert_eq!(output, "        Done crate");
        assert!(!output.contains('\u{1b}'));
    }

    #[test]
    fn action_width_is_configurable() {
        let output = Theme::new(Palette::new(false)).with_action_width(8).action(
            "Done",
            "crate",
            Tone::Success,
        );
        assert_eq!(output, "    Done crate");
    }

    #[test]
    fn enabled_palette_renders_cargo_style_actions_and_headings() {
        let theme = Theme::new(Palette::new(true));
        assert_eq!(
            theme.action("Checking", "crate", Tone::Info),
            "\u{1b}[1m\u{1b}[36m    Checking\u{1b}[0m\u{1b}[0m crate"
        );
        assert_eq!(
            theme.heading("Release plan"),
            "\u{1b}[1mRelease plan\u{1b}[0m"
        );
    }
}
