//! Shared choice/frame rendering for both line-driven and key-driven prompts.
//!
//! One place builds the styled strings a prompt shows, so the numbered list a
//! [`LineTerminal`](super::terminal::LineTerminal) prints and the live frame a
//! `RichTerminal` redraws stay visually
//! consistent. Rendering only *builds* strings — writing and cursor movement are
//! the terminal's job — so these helpers are pure and unit-testable.

use crate::theme::{Glyphs, Palette};

use super::choice::Choice;

/// Styling context shared by every rendered frame: color [`Palette`] and the
/// resolved [`Glyphs`] set.
#[derive(Debug, Clone, Copy)]
pub struct Style {
    palette: Palette,
    glyphs: Glyphs,
}

impl Style {
    /// Bundle a palette and glyph set into a rendering style.
    #[must_use]
    pub const fn new(palette: Palette, glyphs: Glyphs) -> Self {
        Self { palette, glyphs }
    }

    /// The color palette.
    #[must_use]
    pub const fn palette(&self) -> Palette {
        self.palette
    }

    /// The glyph set.
    #[must_use]
    pub const fn glyphs(&self) -> Glyphs {
        self.glyphs
    }
}

/// The bold heading line shown above a prompt's choices or input.
#[must_use]
pub fn heading(style: Style, prompt: &str) -> String {
    style.palette().bold(prompt).into_owned()
}

/// The trailing, dimmed key-hint line for a key-driven frame.
#[must_use]
pub fn key_hint(style: Style, multi: bool) -> String {
    let glyphs = style.glyphs();
    let sep = glyphs.bullet();
    let nav = format!("{}/{} move", glyphs.arrow_up(), glyphs.arrow_down());
    let body = if multi {
        format!("{nav} {sep} space toggle {sep} enter confirm {sep} esc cancel")
    } else {
        format!("{nav} {sep} enter select {sep} esc cancel")
    };
    format!("  {}", style.palette().dim(&body))
}

/// A single choice's label plus its annotation and recommended marker.
fn decorate(style: Style, choice: &Choice, is_default: bool, multi: bool) -> String {
    let mut line = choice.label().to_string();
    if let Some(annotation) = choice.annotation() {
        line.push(' ');
        line.push_str(&style.palette().dim(annotation));
    }
    if choice.is_recommended() {
        line.push(' ');
        line.push_str(&style.palette().info("(recommended)"));
    } else if !multi && is_default {
        line.push(' ');
        line.push_str(&style.palette().info("(default)"));
    }
    line
}

/// Build the static, numbered choice list a line-driven terminal prints once.
///
/// `default` highlights the single-select fallback; it is ignored when `multi`.
#[must_use]
pub fn numbered_rows(
    style: Style,
    choices: &[Choice],
    multi: bool,
    default: Option<usize>,
) -> Vec<String> {
    choices
        .iter()
        .enumerate()
        .map(|(index, choice)| {
            let body = decorate(style, choice, default == Some(index), multi);
            format!("  {}) {body}", index + 1)
        })
        .collect()
}

/// Build the live frame a key-driven terminal redraws as focus and selection
/// change.
///
/// `cursor` is the focused row. For single-select pass `selected` as `None` and
/// each row shows a radio (`Glyphs::radio_on`/`radio_off`); for multi-select pass
/// the chosen indices and each row shows a checkbox (`[x]`/`[ ]`). The focused row
/// is marked with the pointer glyph and rendered bold.
#[must_use]
pub fn frame_rows(
    style: Style,
    choices: &[Choice],
    cursor: usize,
    selected: Option<&[usize]>,
    default: Option<usize>,
) -> Vec<String> {
    let multi = selected.is_some();
    let glyphs = style.glyphs();
    choices
        .iter()
        .enumerate()
        .map(|(index, choice)| {
            let focused = index == cursor;
            let pointer = if focused { glyphs.pointer() } else { " " };
            let marker = match selected {
                Some(chosen) if chosen.contains(&index) => "[x]".to_string(),
                Some(_) => "[ ]".to_string(),
                None if focused => glyphs.radio_on().to_string(),
                None => glyphs.radio_off().to_string(),
            };
            let body = decorate(style, choice, default == Some(index), multi);
            let label = if focused {
                style.palette().bold(&body).into_owned()
            } else {
                body
            };
            format!("{pointer} {marker} {label}")
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::{Style, frame_rows, key_hint, numbered_rows};
    use crate::prompt::choice::Choice;
    use crate::theme::{Glyphs, Palette};

    fn style() -> Style {
        Style::new(Palette::new(false), Glyphs::new(true))
    }

    fn choices() -> Vec<Choice> {
        vec![
            Choice::new("a", "Alpha"),
            Choice::new("b", "Beta").recommended(),
        ]
    }

    #[test]
    fn numbered_rows_are_one_indexed() {
        let rows = numbered_rows(style(), &choices(), false, Some(1));
        assert!(rows[0].starts_with("  1) Alpha"));
        assert!(rows[1].contains("(recommended)"));
    }

    #[test]
    fn frame_rows_show_radio_for_single_select() {
        let rows = frame_rows(style(), &choices(), 0, None, None);
        assert!(rows[0].starts_with("❯ ◉ "));
        assert!(rows[1].starts_with("  ○ "));
    }

    #[test]
    fn frame_rows_show_checkbox_for_multi_select() {
        let rows = frame_rows(style(), &choices(), 1, Some(&[1]), None);
        assert!(rows[0].contains("[ ] "));
        assert!(rows[1].starts_with("❯ [x] "));
    }

    #[test]
    fn ascii_fallback_frames_are_byte_clean() {
        // The ASCII glyph set must keep the rich frame and key hint byte-clean,
        // so a legacy/mis-encoded terminal never renders replacement characters.
        let style = Style::new(Palette::new(false), Glyphs::new(false));
        for row in frame_rows(style, &choices(), 0, None, None) {
            assert!(row.is_ascii(), "radio row must be ASCII: {row:?}");
        }
        for row in frame_rows(style, &choices(), 0, Some(&[0]), None) {
            assert!(row.is_ascii(), "checkbox row must be ASCII: {row:?}");
        }
        assert!(key_hint(style, true).is_ascii());
        assert!(key_hint(style, false).is_ascii());
    }
}
