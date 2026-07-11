//! [`RegionScreen`] — the public seam the renderer talks to.
//!
//! It owns a [`vte::Parser`] and the applied [`Performer`] state and exposes the
//! whole virtual terminal through three methods: [`feed`](RegionScreen::feed)
//! raw bytes and get back the rows that scrolled into scrollback,
//! [`render`](RegionScreen::render) the current tile, and
//! [`drain`](RegionScreen::drain) the remainder when the stream ends. The grid,
//! cursor, and SGR internals stay private behind it.

use super::perform::Performer;

/// A bounded per-region virtual terminal.
///
/// Feed a child's raw output bytes with [`feed`](Self::feed); ANSI cursor,
/// erase, scroll, and color sequences are applied to a fixed `rows × cols` cell
/// grid rather than passed through, so a child that redraws in place renders
/// correctly and can never move the host terminal's cursor. Rows that scroll off
/// the top are returned for the caller to flush to scrollback, preserving the
/// complete transcript.
pub struct RegionScreen {
    parser: vte::Parser,
    performer: Performer,
}

impl std::fmt::Debug for RegionScreen {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        // `vte::Parser` is opaque and carries no state worth showing.
        f.debug_struct("RegionScreen")
            .field("performer", &self.performer)
            .finish_non_exhaustive()
    }
}

impl RegionScreen {
    /// Create a screen over a fresh `rows × cols` grid (each clamped to ≥ 1).
    #[must_use]
    pub fn new(rows: usize, cols: usize) -> Self {
        Self {
            parser: vte::Parser::new(),
            performer: Performer::new(rows, cols),
        }
    }

    /// Feed raw output bytes, returning the rows evicted by scrolling.
    ///
    /// Bytes are parsed as a VT stream and applied to the grid; multi-byte and
    /// split UTF-8 sequences are reassembled across calls by the parser. Evicted
    /// rows are returned oldest first, already styled, so the caller can print
    /// them into scrollback in order.
    pub fn feed(&mut self, bytes: &[u8]) -> Vec<String> {
        self.parser.advance(&mut self.performer, bytes);
        self.performer.take_evicted()
    }

    /// The current grid contents as styled rows (exactly the grid height).
    #[must_use]
    pub fn render(&self) -> Vec<String> {
        self.performer.render()
    }

    /// Consume the screen, returning its remaining rows with trailing blank
    /// rows trimmed, so evicted rows plus this drain reconstruct the transcript.
    #[must_use]
    pub fn drain(self) -> Vec<String> {
        let mut rows = self.performer.render();
        while rows.last().is_some_and(String::is_empty) {
            rows.pop();
        }
        rows
    }
}

#[cfg(test)]
mod tests {
    use super::RegionScreen;

    #[test]
    fn carriage_return_progress_collapses() {
        let mut screen = RegionScreen::new(1, 8);
        assert!(screen.feed(b"50%\r100%").is_empty());
        assert_eq!(screen.render(), vec!["100%".to_string()]);
    }

    #[test]
    fn utf8_split_across_feeds_is_reassembled() {
        let mut screen = RegionScreen::new(1, 4);
        assert!(screen.feed(&[0xC3]).is_empty());
        assert!(screen.feed(&[0xA9]).is_empty());
        assert_eq!(screen.render(), vec!["é".to_string()]);
    }

    #[test]
    fn in_place_multiline_redraw_updates_block_without_duplication() {
        let mut screen = RegionScreen::new(3, 8);
        screen.feed(b"a\r\nb\r\nc");
        // Jump home and rewrite the whole block in place.
        screen.feed(b"\x1b[H\x1b[2Jx\r\ny\r\nz");
        assert_eq!(
            screen.render(),
            vec!["x".to_string(), "y".to_string(), "z".to_string()]
        );
    }

    #[test]
    fn regression_cargo_style_redraw_leaks_no_control_bytes() {
        let mut screen = RegionScreen::new(6, 20);
        // A curses-style frame: erase-line + cursor-up/down that would corrupt a
        // pass-through line buffer. The grid absorbs it into plain content.
        let frame = b"Compiling\r\n\x1b[7A\r\x1b[2K\x1b[1BChecking\r\nDone";
        let evicted = screen.feed(frame);
        let mut rendered = evicted;
        rendered.extend(screen.render());
        for line in &rendered {
            assert!(!line.contains("\x1b[2K"));
            assert!(!line.contains("\x1b[7A"));
            assert!(!line.contains("\x1b[1B"));
        }
    }

    #[test]
    fn evicted_plus_drain_reconstruct_full_transcript() {
        let mut screen = RegionScreen::new(2, 4);
        let mut all = Vec::new();
        all.extend(screen.feed(b"l1\r\nl2\r\nl3\r\nl4\r\n"));
        all.extend(screen.drain());
        assert_eq!(all, vec!["l1", "l2", "l3", "l4"]);
    }

    #[test]
    fn drain_trims_trailing_blank_rows() {
        let mut screen = RegionScreen::new(5, 8);
        screen.feed(b"one\r\ntwo");
        assert_eq!(screen.drain(), vec!["one".to_string(), "two".to_string()]);
    }
}
