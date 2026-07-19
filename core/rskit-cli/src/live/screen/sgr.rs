//! [`Sgr`] — the color and attribute state carried by every grid cell.
//!
//! This isolates ANSI *Select Graphic Rendition* handling:
//! parsing the `m` parameters a child emits into a compact per-cell state,
//! and serializing a row of styled cells back into a minimal ANSI string for the renderer.
//! Keeping it separate lets both the tile render
//! and the scrollback-eviction render share one styling implementation.

use super::grid::Cell;

/// A single foreground or background color.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub(super) enum Color {
    /// The terminal's default color (no SGR color code emitted).
    #[default]
    Default,
    /// One of the 256 palette colors (0-15 are the 16 ANSI colors).
    Indexed(u8),
    /// A 24-bit truecolor value.
    Rgb(u8, u8, u8),
}

/// The graphic-rendition state applied to a cell: colors plus attributes.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
#[allow(
    clippy::struct_excessive_bools,
    reason = "each flag is an independent SGR attribute"
)]
pub(super) struct Sgr {
    fg: Color,
    bg: Color,
    bold: bool,
    dim: bool,
    italic: bool,
    underline: bool,
    reverse: bool,
}

/// The SGR reset sequence,
/// emitted to close a rendered row only when styling is still active at its end (a row ending in default state needs no reset).
const RESET: &str = "\x1b[0m";

/// Narrow an SGR parameter to a byte, saturating out-of-range values.
fn byte(value: u16) -> u8 {
    u8::try_from(value).unwrap_or(u8::MAX)
}

impl Sgr {
    /// The rendition an erase leaves behind:
    /// a blank keeps the active background (background-color erase, as VT/xterm do) while dropping the foreground
    /// and attributes,
    /// so `EL`/`ED` under a colored background fill with that background instead of the terminal default.
    pub(super) fn erased(self) -> Self {
        Self {
            bg: self.bg,
            ..Self::default()
        }
    }

    /// Apply an SGR (`CSI … m`) parameter list, mutating this state in place.
    ///
    /// Handles the 16-color, 256-color (`38;5;n` / `38:5:n`),
    /// and truecolor (`38;2;r;g;b` / `38:2:r:g:b`) forms, in both the semicolon
    /// and colon sub-parameter encodings, plus the common attribute toggles.
    /// An empty parameter list resets to the default state, matching a bare `CSI m`.
    pub(super) fn apply(&mut self, params: &vte::Params) {
        if params.is_empty() {
            *self = Self::default();
            return;
        }
        let mut iter = params.iter();
        while let Some(param) = iter.next() {
            let code = param.first().copied().unwrap_or(0);
            match code {
                0 => *self = Self::default(),
                1 => self.bold = true,
                2 => self.dim = true,
                3 => self.italic = true,
                4 => self.underline = true,
                7 => self.reverse = true,
                22 => {
                    self.bold = false;
                    self.dim = false;
                }
                23 => self.italic = false,
                24 => self.underline = false,
                27 => self.reverse = false,
                30..=37 => self.fg = Color::Indexed(byte(code - 30)),
                39 => self.fg = Color::Default,
                40..=47 => self.bg = Color::Indexed(byte(code - 40)),
                49 => self.bg = Color::Default,
                90..=97 => self.fg = Color::Indexed(byte(code - 90 + 8)),
                100..=107 => self.bg = Color::Indexed(byte(code - 100 + 8)),
                38 => {
                    if let Some(color) = extended_color(param, &mut iter) {
                        self.fg = color;
                    }
                }
                48 => {
                    if let Some(color) = extended_color(param, &mut iter) {
                        self.bg = color;
                    }
                }
                _ => {}
            }
        }
    }

    /// The full `CSI … m` sequence that establishes this state from the reset default,
    /// always leading with a `0` reset so it is self-contained.
    fn escape(&self) -> String {
        let mut codes: Vec<String> = vec!["0".to_string()];
        if self.bold {
            codes.push("1".to_string());
        }
        if self.dim {
            codes.push("2".to_string());
        }
        if self.italic {
            codes.push("3".to_string());
        }
        if self.underline {
            codes.push("4".to_string());
        }
        if self.reverse {
            codes.push("7".to_string());
        }
        push_color(&mut codes, self.fg, false);
        push_color(&mut codes, self.bg, true);
        format!("\x1b[{}m", codes.join(";"))
    }
}

/// Append the SGR codes for one color to `codes`;
/// `background` selects the 40/48/100 range over the 30/38/90 range. A default color emits nothing.
fn push_color(codes: &mut Vec<String>, color: Color, background: bool) {
    let (base, bright_base, extended) = if background {
        (40u16, 100u16, 48u16)
    } else {
        (30, 90, 38)
    };
    match color {
        Color::Default => {}
        Color::Indexed(index @ 0..=7) => codes.push((base + u16::from(index)).to_string()),
        Color::Indexed(index @ 8..=15) => {
            codes.push((bright_base + u16::from(index) - 8).to_string());
        }
        Color::Indexed(index) => {
            codes.push(format!("{extended};5;{index}"));
        }
        Color::Rgb(r, g, b) => codes.push(format!("{extended};2;{r};{g};{b}")),
    }
}

/// Decode a `38`/`48` extended-color argument in either the colon form (all sub-parameters inside `param`)
/// or the semicolon form (mode and channels in following parameters, pulled from `iter`).
fn extended_color<'a>(param: &[u16], iter: &mut impl Iterator<Item = &'a [u16]>) -> Option<Color> {
    if param.len() > 1 {
        return match param.get(1)? {
            5 => param.get(2).map(|&index| Color::Indexed(byte(index))),
            2 => {
                // Some encoders insert a color-space id (`38:2::r:g:b`),
                // so take the final three sub-parameters as the RGB channels.
                let len = param.len();
                if len < 5 {
                    return None;
                }
                Some(Color::Rgb(
                    byte(param[len - 3]),
                    byte(param[len - 2]),
                    byte(param[len - 1]),
                ))
            }
            _ => None,
        };
    }
    match iter.next()?.first()? {
        5 => iter
            .next()
            .and_then(<[u16]>::first)
            .map(|&index| Color::Indexed(byte(index))),
        2 => {
            let r = byte(*iter.next()?.first()?);
            let g = byte(*iter.next()?.first()?);
            let b = byte(*iter.next()?.first()?);
            Some(Color::Rgb(r, g, b))
        }
        _ => None,
    }
}

/// Serialize one grid row into a styled string with minimal SGR transitions.
///
/// Trailing default cells are trimmed so blank tail space carries no styling,
/// an SGR sequence is emitted only where a cell's state differs from the previous one,
/// and a single reset closes the row only when styling is still active at its end —
/// a row that returns to default state needs no reset.
pub(super) fn render_row(cells: &[Cell]) -> String {
    let end = cells
        .iter()
        .rposition(|cell| !cell.is_blank())
        .map_or(0, |index| index + 1);
    let mut out = String::new();
    let mut current = Sgr::default();
    for cell in &cells[..end] {
        if cell.sgr != current {
            out.push_str(&cell.sgr.escape());
            current = cell.sgr;
        }
        out.push(cell.ch);
    }
    if current != Sgr::default() {
        out.push_str(RESET);
    }
    out
}

#[cfg(test)]
mod tests {
    use super::super::grid::Cell;
    use super::{Color, Sgr, render_row};

    fn styled_cells(input: &[u8]) -> Vec<Cell> {
        let mut parser = vte::Parser::new();
        let mut row = Row::default();
        parser.advance(&mut row, input);
        row.cells
    }

    #[derive(Default)]
    struct Row {
        sgr: Sgr,
        cells: Vec<Cell>,
    }

    impl vte::Perform for Row {
        fn print(&mut self, c: char) {
            self.cells.push(Cell {
                ch: c,
                sgr: self.sgr,
            });
        }

        fn csi_dispatch(
            &mut self,
            params: &vte::Params,
            _intermediates: &[u8],
            _ignore: bool,
            action: char,
        ) {
            if action == 'm' {
                self.sgr.apply(params);
            }
        }
    }

    #[test]
    fn parses_basic_colors_and_attributes() {
        let cells = styled_cells(b"\x1b[1;31mA\x1b[0mB");
        assert_eq!(cells[0].sgr.fg, Color::Indexed(1));
        assert!(cells[0].sgr.bold);
        assert_eq!(cells[1].sgr, Sgr::default());
    }

    #[test]
    fn parses_256_and_truecolor_in_both_forms() {
        assert_eq!(
            styled_cells(b"\x1b[38;5;208mX")[0].sgr.fg,
            Color::Indexed(208)
        );
        assert_eq!(
            styled_cells(b"\x1b[38:5:208mX")[0].sgr.fg,
            Color::Indexed(208)
        );
        assert_eq!(
            styled_cells(b"\x1b[38;2;10;20;30mX")[0].sgr.fg,
            Color::Rgb(10, 20, 30)
        );
        assert_eq!(
            styled_cells(b"\x1b[38:2::10:20:30mX")[0].sgr.fg,
            Color::Rgb(10, 20, 30)
        );
    }

    #[test]
    fn render_row_emits_minimal_transitions_and_no_redundant_reset() {
        // Styling returns to default before the row ends, so no trailing reset.
        let cells = styled_cells(b"\x1b[31mAB\x1b[0mC");
        assert_eq!(render_row(&cells), "\x1b[0;31mAB\x1b[0mC");
    }

    #[test]
    fn render_row_closes_row_when_styling_remains_active() {
        // Styling is still active at the last cell, so a reset closes the row.
        let cells = styled_cells(b"\x1b[31mAB");
        assert_eq!(render_row(&cells), "\x1b[0;31mAB\x1b[0m");
    }

    #[test]
    fn render_row_trims_trailing_blanks() {
        let mut cells = styled_cells(b"hi");
        cells.extend(std::iter::repeat_n(Cell::default(), 4));
        assert_eq!(render_row(&cells), "hi");
    }

    #[test]
    fn color_survives_round_trip() {
        let cells = styled_cells(b"\x1b[38;5;123mZ");
        let rendered = render_row(&cells);
        let reparsed = styled_cells(rendered.as_bytes());
        assert_eq!(reparsed[0].sgr.fg, Color::Indexed(123));
    }
}
