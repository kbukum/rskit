//! [`Performer`] — maps the parsed VT stream onto the grid, cursor, and SGR.
//!
//! This is the only file that depends on [`vte`]: it implements
//! [`vte::Perform`], translating each parsed print, C0 control, and CSI dispatch
//! into a grid write, a cursor move, an erase, a scroll, or an SGR update.
//! Swapping the parser (or a hand-rolled fallback) touches this file alone.

use super::cursor::Cursor;
use super::grid::{Cell, EraseMode, Grid};
use super::sgr::Sgr;

/// The applied state of a region's virtual terminal.
///
/// Holds the cell grid, the write cursor, the current SGR, and the rows evicted
/// by scrolling since the last drain. It is driven by a [`vte::Parser`] and read
/// back through the grid.
#[derive(Debug)]
pub(super) struct Performer {
    grid: Grid,
    cursor: Cursor,
    sgr: Sgr,
    evicted: Vec<String>,
}

impl Performer {
    /// Create a performer over a fresh `rows × cols` grid.
    pub(super) fn new(rows: usize, cols: usize) -> Self {
        Self {
            grid: Grid::new(rows, cols),
            cursor: Cursor::default(),
            sgr: Sgr::default(),
            evicted: Vec::new(),
        }
    }

    /// Take the rows evicted by scrolling since the last call (oldest first).
    pub(super) fn take_evicted(&mut self) -> Vec<String> {
        std::mem::take(&mut self.evicted)
    }

    /// The current grid rows rendered as styled strings (fixed height).
    pub(super) fn render(&self) -> Vec<String> {
        self.grid.render()
    }

    /// Line-feed the cursor, capturing any evicted top row.
    fn line_feed(&mut self) {
        if let Some(row) = self.grid.line_feed(&mut self.cursor) {
            self.evicted.push(row);
        }
    }

    /// Read the `index`-th CSI parameter (zero-based), falling back to `default`
    /// when it is absent or explicitly zero.
    fn param(params: &vte::Params, index: usize, default: usize) -> usize {
        params
            .iter()
            .nth(index)
            .and_then(<[u16]>::first)
            .map(|&value| value as usize)
            .filter(|&value| value != 0)
            .unwrap_or(default)
    }
}

impl vte::Perform for Performer {
    fn print(&mut self, c: char) {
        if self.cursor.col >= self.grid.cols() {
            self.cursor.carriage_return();
            self.line_feed();
        }
        self.grid.set(
            self.cursor.row,
            self.cursor.col,
            Cell {
                ch: c,
                sgr: self.sgr,
            },
        );
        self.cursor.col += 1;
    }

    fn execute(&mut self, byte: u8) {
        // Applying a control from the pending-wrap sentinel would make the next
        // print wrap and possibly scroll; land back inside the grid first.
        self.cursor.clear_pending_wrap(self.grid.cols());
        match byte {
            // Region output arrives over a pipe, so the terminal's ONLCR
            // translation never runs: cook a bare `\n` into carriage-return +
            // line-feed so plain `\n`-terminated output does not staircase.
            b'\n' => {
                self.cursor.carriage_return();
                self.line_feed();
            }
            b'\r' => self.cursor.carriage_return(),
            b'\t' => self.cursor.tab(self.grid.cols()),
            0x08 => self.cursor.backspace(),
            _ => {}
        }
    }

    fn csi_dispatch(
        &mut self,
        params: &vte::Params,
        _intermediates: &[u8],
        _ignore: bool,
        action: char,
    ) {
        let rows = self.grid.rows();
        let cols = self.grid.cols();
        // Explicit moves clear a pending wrap, matching terminal behavior and
        // preventing a stale `col == cols` from scrolling on the next print.
        self.cursor.clear_pending_wrap(cols);
        match action {
            'A' => self.cursor.up(Self::param(params, 0, 1)),
            'B' => self.cursor.down(Self::param(params, 0, 1), rows),
            'C' => self.cursor.right(Self::param(params, 0, 1), cols),
            'D' => self.cursor.left(Self::param(params, 0, 1)),
            'H' | 'f' => {
                let row = Self::param(params, 0, 1);
                let col = Self::param(params, 1, 1);
                self.cursor.to(row, col, rows, cols);
            }
            'K' => {
                if let Some(mode) = EraseMode::from_param(Self::param(params, 0, 0)) {
                    self.grid
                        .erase_line(&self.cursor, mode, &Cell::blank(self.sgr.erased()));
                }
            }
            'J' => {
                if let Some(mode) = EraseMode::from_param(Self::param(params, 0, 0)) {
                    self.grid
                        .erase_display(&self.cursor, mode, &Cell::blank(self.sgr.erased()));
                }
            }
            'S' => {
                let evicted = self.grid.scroll_up(Self::param(params, 0, 1));
                self.evicted.extend(evicted);
            }
            'T' => self.grid.scroll_down(Self::param(params, 0, 1)),
            'm' => self.sgr.apply(params),
            _ => {}
        }
    }
}

#[cfg(test)]
mod tests {
    use super::Performer;

    fn feed(performer: &mut Performer, bytes: &[u8]) {
        let mut parser = vte::Parser::new();
        parser.advance(performer, bytes);
    }

    #[test]
    fn print_wraps_at_width() {
        let mut performer = Performer::new(2, 3);
        feed(&mut performer, b"abcd");
        assert_eq!(performer.render(), vec!["abc".to_string(), "d".to_string()]);
    }

    #[test]
    fn carriage_return_overwrites_in_place() {
        let mut performer = Performer::new(1, 8);
        feed(&mut performer, b"50%\r100%");
        assert_eq!(performer.render(), vec!["100%".to_string()]);
    }

    #[test]
    fn cursor_up_and_erase_line_redraw_in_place() {
        let mut performer = Performer::new(3, 6);
        feed(&mut performer, b"one\r\ntwo\r\n");
        // Move up two rows, clear the line, and rewrite it.
        feed(&mut performer, b"\x1b[2A\x1b[2Kzero");
        assert_eq!(
            performer.render(),
            vec!["zero".to_string(), "two".to_string(), String::new()]
        );
    }

    #[test]
    fn line_feed_at_bottom_evicts_top_row() {
        let mut performer = Performer::new(2, 4);
        feed(&mut performer, b"a\r\nb\r\nc");
        assert_eq!(performer.take_evicted(), vec!["a".to_string()]);
        assert_eq!(performer.render(), vec!["b".to_string(), "c".to_string()]);
    }

    #[test]
    fn control_sequences_never_leak_into_content() {
        let mut performer = Performer::new(3, 10);
        // A cargo-style in-place redraw frame: move up, erase, move down.
        feed(&mut performer, b"building\r\n\x1b[1A\r\x1b[2K\x1b[1Bdone");
        let rendered = performer.render();
        // This frame emits no SGR, so no ESC byte may survive into content.
        for line in &rendered {
            assert!(
                !line.contains('\x1b'),
                "escape leaked into content: {line:?}"
            );
        }
        // No stray escape control chars beyond well-formed SGR spans.
        assert!(
            !rendered
                .iter()
                .any(|line| line.contains("[2K") || line.contains("[1A"))
        );
    }

    #[test]
    fn bare_line_feed_returns_to_column_zero() {
        // Piped output has no ONLCR, so a bare `\n` must not staircase.
        let mut performer = Performer::new(2, 6);
        feed(&mut performer, b"one\ntwo");
        assert_eq!(
            performer.render(),
            vec!["one".to_string(), "two".to_string()]
        );
    }

    #[test]
    fn cursor_move_from_pending_wrap_does_not_scroll() {
        // At the bottom row with a filled last column (col == cols sentinel), a
        // cursor-down (clamped to the bottom) must clear the pending wrap so the
        // next print overwrites in place instead of wrapping and scrolling.
        let mut performer = Performer::new(2, 3);
        feed(&mut performer, b"x\r\nabc\x1b[BY");
        assert_eq!(
            performer.take_evicted(),
            Vec::<String>::new(),
            "explicit move from pending wrap must not evict rows"
        );
        assert_eq!(performer.render(), vec!["x".to_string(), "abY".to_string()]);
    }

    #[test]
    fn erase_preserves_active_background() {
        // A child sets a background then erases to end of line: the erased span
        // must keep that background (BCE), not fall back to the default.
        let mut performer = Performer::new(1, 4);
        feed(&mut performer, b"\x1b[41mAB\x1b[2K");
        let rendered = performer.render();
        // The whole row is the red-background erase fill.
        assert_eq!(rendered, vec!["\x1b[0;41m    \x1b[0m".to_string()]);
    }

    #[test]
    fn osc_sequences_are_ignored() {
        let mut performer = Performer::new(1, 10);
        feed(&mut performer, b"\x1b]0;title\x07hi");
        assert_eq!(performer.render(), vec!["hi".to_string()]);
    }
}
