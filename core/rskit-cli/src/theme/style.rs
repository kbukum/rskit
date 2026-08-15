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
    ///
    /// The label is right-aligned to [`action_width`](Self::with_action_width) *terminal columns* (via [`console::measure_text_width`]), so wide (CJK / emoji) or combining labels stay aligned rather than being padded by Unicode scalar count.
    #[must_use]
    pub fn action(self, label: &str, detail: &str, tone: Tone) -> String {
        let pad = self
            .action_width
            .saturating_sub(console::measure_text_width(label));
        let aligned = format!("{blank:pad$}{label}", blank = "");
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
    fn wide_labels_align_to_terminal_columns_not_scalar_count() {
        // "構築" is two scalars but four terminal columns, so a 12-column field
        // leaves eight spaces — a scalar-count pad would wrongly leave ten.
        let output = Theme::new(Palette::new(false)).action("構築", "crate", Tone::Info);
        assert_eq!(output, "        構築 crate");
    }

    #[test]
    fn overlong_label_is_not_truncated_and_gets_no_padding() {
        let output = Theme::new(Palette::new(false)).with_action_width(4).action(
            "Compiling",
            "crate",
            Tone::Info,
        );
        assert_eq!(output, "Compiling crate");
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
