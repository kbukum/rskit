//! [`Cursor`] — the bounded write position within a region's grid.
//!
//! All movement is pure position math with clamping to the grid geometry, kept
//! here so the parser's [`Perform`](super::perform) mapping never has to reason
//! about bounds. The cursor column may transiently equal `cols` to model the
//! deferred wrap of a just-filled last column; explicit moves always land back
//! inside the grid.

/// The write position: a zero-based `(row, col)` into the grid.
#[derive(Debug, Clone, Copy, Default)]
pub(super) struct Cursor {
    /// Current row, always in `0..rows`.
    pub row: usize,
    /// Current column, in `0..=cols` (`cols` is the pending-wrap sentinel).
    pub col: usize,
}

impl Cursor {
    /// Move up `n` rows, clamped at the top.
    pub(super) fn up(&mut self, n: usize) {
        self.row = self.row.saturating_sub(n.max(1));
    }

    /// Move down `n` rows, clamped at the last row.
    pub(super) fn down(&mut self, n: usize, rows: usize) {
        self.row = (self.row + n.max(1)).min(rows.saturating_sub(1));
    }

    /// Move right `n` columns, clamped at the last column.
    pub(super) fn right(&mut self, n: usize, cols: usize) {
        self.col = (self.col + n.max(1)).min(cols.saturating_sub(1));
    }

    /// Move left `n` columns, clamped at column zero.
    pub(super) fn left(&mut self, n: usize) {
        self.col = self.col.saturating_sub(n.max(1));
    }

    /// Jump to an absolute one-based `(row, col)`, clamped to the grid.
    pub(super) fn to(&mut self, row: usize, col: usize, rows: usize, cols: usize) {
        self.row = row.saturating_sub(1).min(rows.saturating_sub(1));
        self.col = col.saturating_sub(1).min(cols.saturating_sub(1));
    }

    /// Return to column zero (carriage return).
    pub(super) const fn carriage_return(&mut self) {
        self.col = 0;
    }

    /// Advance to the next eight-column tab stop, clamped at the last column.
    pub(super) fn tab(&mut self, cols: usize) {
        let next = (self.col / 8 + 1) * 8;
        self.col = next.min(cols.saturating_sub(1));
    }

    /// Move left one column (backspace), clamped at column zero.
    pub(super) const fn backspace(&mut self) {
        self.col = self.col.saturating_sub(1);
    }
}

#[cfg(test)]
mod tests {
    use super::Cursor;

    #[test]
    fn moves_clamp_at_edges() {
        let mut cursor = Cursor { row: 0, col: 0 };
        cursor.up(3);
        cursor.left(3);
        assert_eq!((cursor.row, cursor.col), (0, 0));
        cursor.down(10, 4);
        cursor.right(10, 5);
        assert_eq!((cursor.row, cursor.col), (3, 4));
    }

    #[test]
    fn absolute_move_is_one_based_and_clamped() {
        let mut cursor = Cursor::default();
        cursor.to(2, 3, 5, 5);
        assert_eq!((cursor.row, cursor.col), (1, 2));
        cursor.to(99, 99, 4, 4);
        assert_eq!((cursor.row, cursor.col), (3, 3));
    }

    #[test]
    fn absent_param_defaults_to_one() {
        let mut cursor = Cursor { row: 3, col: 3 };
        cursor.up(0);
        assert_eq!(cursor.row, 2);
    }

    #[test]
    fn tab_stops_and_backspace() {
        let mut cursor = Cursor { row: 0, col: 0 };
        cursor.tab(80);
        assert_eq!(cursor.col, 8);
        cursor.col = 10;
        cursor.tab(80);
        assert_eq!(cursor.col, 16);
        cursor.backspace();
        assert_eq!(cursor.col, 15);
    }

    #[test]
    fn tab_clamps_to_last_column() {
        let mut cursor = Cursor { row: 0, col: 6 };
        cursor.tab(8);
        assert_eq!(cursor.col, 7);
    }
}
