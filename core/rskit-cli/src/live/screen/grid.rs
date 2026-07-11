//! [`Grid`] — the fixed `rows × cols` cell store behind a region tile.
//!
//! This is pure data and geometry: it holds the cell matrix, writes cells,
//! erases spans, and scrolls. Scrolling off the top emits the evicted row as a
//! styled string so the region can flush the complete transcript to scrollback.
//! It knows nothing about ANSI parsing or I/O, which keeps it the most heavily
//! unit-tested unit of the virtual terminal.

use std::collections::VecDeque;

use super::cursor::Cursor;
use super::sgr::{Sgr, render_row};

/// One character cell: its glyph and the graphic-rendition state it carries.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct Cell {
    /// The glyph shown in this cell.
    pub ch: char,
    /// The color/attribute state applied to the glyph.
    pub sgr: Sgr,
}

impl Default for Cell {
    fn default() -> Self {
        Self {
            ch: ' ',
            sgr: Sgr::default(),
        }
    }
}

impl Cell {
    /// Whether this cell is a blank: a space with default styling.
    pub(super) fn is_blank(&self) -> bool {
        self.ch == ' ' && self.sgr == Sgr::default()
    }
}

/// Which cells an erase (`EL`/`ED`) clears relative to the cursor.
#[derive(Debug, Clone, Copy)]
pub(super) enum EraseMode {
    /// From the cursor to the end (mode 0).
    ToEnd,
    /// From the start to the cursor inclusive (mode 1).
    ToStart,
    /// The entire line or display (mode 2).
    All,
}

impl EraseMode {
    /// Map a CSI numeric parameter to an erase mode, ignoring unknown values.
    pub(super) const fn from_param(param: usize) -> Option<Self> {
        match param {
            0 => Some(Self::ToEnd),
            1 => Some(Self::ToStart),
            2 => Some(Self::All),
            _ => None,
        }
    }
}

/// A fixed-height cell matrix with erase and scroll operations.
///
/// Rows are held in a [`VecDeque`] so scrolling is O(1) at either end
/// (`pop_front`/`push_back` to scroll up, `pop_back`/`push_front` to scroll
/// down) rather than shifting every row.
#[derive(Debug)]
pub(super) struct Grid {
    rows: usize,
    cols: usize,
    lines: VecDeque<Vec<Cell>>,
}

impl Grid {
    /// Create a blank grid of `rows × cols`, each dimension clamped to at least
    /// one so geometry math never underflows.
    pub(super) fn new(rows: usize, cols: usize) -> Self {
        let rows = rows.max(1);
        let cols = cols.max(1);
        Self {
            rows,
            cols,
            lines: std::iter::repeat_with(|| vec![Cell::default(); cols])
                .take(rows)
                .collect(),
        }
    }

    /// Number of rows.
    pub(super) const fn rows(&self) -> usize {
        self.rows
    }

    /// Number of columns.
    pub(super) const fn cols(&self) -> usize {
        self.cols
    }

    /// Write `cell` at `(row, col)`, ignoring positions outside the grid.
    pub(super) fn set(&mut self, row: usize, col: usize, cell: Cell) {
        if let Some(target) = self.lines.get_mut(row).and_then(|line| line.get_mut(col)) {
            *target = cell;
        }
    }

    /// Line-feed at `cursor`: advance one row, scrolling up (and returning the
    /// evicted top row) when already on the last row.
    pub(super) fn line_feed(&mut self, cursor: &mut Cursor) -> Option<String> {
        if cursor.row + 1 < self.rows {
            cursor.row += 1;
            None
        } else {
            self.scroll_up(1).into_iter().next()
        }
    }

    /// Scroll the whole grid up `n` rows, returning the evicted top rows
    /// (oldest first) as styled strings and back-filling blanks at the bottom.
    pub(super) fn scroll_up(&mut self, n: usize) -> Vec<String> {
        let n = n.max(1).min(self.rows);
        let mut evicted = Vec::with_capacity(n);
        for _ in 0..n {
            if let Some(line) = self.lines.pop_front() {
                evicted.push(render_row(&line));
            }
            self.lines.push_back(vec![Cell::default(); self.cols]);
        }
        evicted
    }

    /// Scroll the whole grid down `n` rows, inserting blanks at the top and
    /// dropping the bottom rows. Rows pushed off the bottom are not scrollback.
    pub(super) fn scroll_down(&mut self, n: usize) {
        let n = n.max(1).min(self.rows);
        for _ in 0..n {
            self.lines.pop_back();
            self.lines.push_front(vec![Cell::default(); self.cols]);
        }
    }

    /// Erase within the cursor's row per `mode`.
    pub(super) fn erase_line(&mut self, cursor: &Cursor, mode: EraseMode) {
        let cols = self.cols;
        let Some(line) = self.lines.get_mut(cursor.row) else {
            return;
        };
        let range = match mode {
            EraseMode::ToEnd => cursor.col.min(cols)..cols,
            EraseMode::ToStart => 0..(cursor.col + 1).min(cols),
            EraseMode::All => 0..cols,
        };
        for cell in &mut line[range] {
            *cell = Cell::default();
        }
    }

    /// Erase within the whole display per `mode`, relative to the cursor.
    pub(super) fn erase_display(&mut self, cursor: &Cursor, mode: EraseMode) {
        match mode {
            EraseMode::ToEnd => {
                self.erase_line(cursor, EraseMode::ToEnd);
                self.clear_rows(cursor.row + 1..self.rows);
            }
            EraseMode::ToStart => {
                self.clear_rows(0..cursor.row);
                self.erase_line(cursor, EraseMode::ToStart);
            }
            EraseMode::All => self.clear_rows(0..self.rows),
        }
    }

    /// Reset every cell in the given row range to a blank.
    fn clear_rows(&mut self, range: std::ops::Range<usize>) {
        for row in range {
            if let Some(line) = self.lines.get_mut(row) {
                for cell in line {
                    *cell = Cell::default();
                }
            }
        }
    }

    /// Render every row to a styled string (fixed height, oldest at the top).
    pub(super) fn render(&self) -> Vec<String> {
        self.lines.iter().map(|line| render_row(line)).collect()
    }
}

#[cfg(test)]
mod tests {
    use super::super::cursor::Cursor;
    use super::{Cell, EraseMode, Grid};

    fn write(grid: &mut Grid, row: usize, text: &str) {
        for (col, ch) in text.chars().enumerate() {
            grid.set(
                row,
                col,
                Cell {
                    ch,
                    ..Cell::default()
                },
            );
        }
    }

    #[test]
    fn scroll_up_evicts_top_row_and_backfills_blank() {
        let mut grid = Grid::new(2, 4);
        write(&mut grid, 0, "aa");
        write(&mut grid, 1, "bb");
        let evicted = grid.scroll_up(1);
        assert_eq!(evicted, vec!["aa".to_string()]);
        assert_eq!(grid.render(), vec!["bb".to_string(), String::new()]);
    }

    #[test]
    fn line_feed_scrolls_only_at_the_bottom() {
        let mut grid = Grid::new(2, 4);
        let mut cursor = Cursor::default();
        assert!(grid.line_feed(&mut cursor).is_none());
        assert_eq!(cursor.row, 1);
        write(&mut grid, 1, "xx");
        assert_eq!(grid.line_feed(&mut cursor).as_deref(), Some(""));
        assert_eq!(cursor.row, 1);
        assert_eq!(grid.render(), vec!["xx".to_string(), String::new()]);
    }

    #[test]
    fn erase_line_modes_clear_the_right_span() {
        let mut grid = Grid::new(1, 5);
        write(&mut grid, 0, "abcde");
        let cursor = Cursor { row: 0, col: 2 };
        grid.erase_line(&cursor, EraseMode::ToEnd);
        assert_eq!(grid.render(), vec!["ab".to_string()]);

        write(&mut grid, 0, "abcde");
        grid.erase_line(&cursor, EraseMode::ToStart);
        assert_eq!(grid.render(), vec!["   de".to_string()]);
    }

    #[test]
    fn erase_display_to_end_clears_below() {
        let mut grid = Grid::new(3, 3);
        write(&mut grid, 0, "aaa");
        write(&mut grid, 1, "bbb");
        write(&mut grid, 2, "ccc");
        let cursor = Cursor { row: 1, col: 1 };
        grid.erase_display(&cursor, EraseMode::ToEnd);
        assert_eq!(
            grid.render(),
            vec!["aaa".to_string(), "b".to_string(), String::new()]
        );
    }

    #[test]
    fn render_trims_trailing_blanks_per_row() {
        let mut grid = Grid::new(1, 6);
        write(&mut grid, 0, "hi");
        assert_eq!(grid.render(), vec!["hi".to_string()]);
    }
}
