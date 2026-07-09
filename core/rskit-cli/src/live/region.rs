//! [`RegionTail`] — a bounded viewport over one stream of raw output bytes.
//!
//! A region shows only the last *k* lines of its stream (a "tile"). As new
//! lines push older ones out of that window, the evicted lines are handed back
//! to the caller to flush into scrollback, so the live area stays a fixed
//! height while nothing is lost. This is pure line-accounting with no terminal
//! I/O, which keeps it deterministic and testable independent of any renderer.

use std::collections::VecDeque;

/// A fixed-height viewport over a byte stream, tracking the last *k* lines.
///
/// Feed raw bytes with [`push_bytes`](Self::push_bytes); it returns the lines
/// that scrolled out of the window (oldest first) for the caller to flush.
/// [`visible`](Self::visible) is the current tile contents, and
/// [`drain`](Self::drain) yields whatever remains when the stream ends.
///
/// Carriage returns (`\r`) reset the current line in place, matching a terminal
/// line discipline so progress-bar redraws collapse to their final state rather
/// than accumulating.
#[derive(Debug)]
pub struct RegionTail {
    /// Completed lines currently shown above the in-progress line.
    completed: VecDeque<String>,
    /// The in-progress line (no trailing newline yet).
    current: String,
    /// Maximum number of visible lines (completed + the in-progress line).
    height: usize,
}

impl RegionTail {
    /// Create a tail showing at most `height` lines (clamped to at least one).
    #[must_use]
    pub fn new(height: usize) -> Self {
        Self {
            completed: VecDeque::new(),
            current: String::new(),
            height: height.max(1),
        }
    }

    /// Append raw bytes, returning any lines evicted from the visible window.
    ///
    /// Bytes are decoded lossily so invalid UTF-8 never aborts rendering; ANSI
    /// styling bytes pass through untouched. Evicted lines are returned oldest
    /// first so the caller can print them into scrollback in order.
    pub fn push_bytes(&mut self, bytes: &[u8]) -> Vec<String> {
        let text = String::from_utf8_lossy(bytes);
        let mut evicted = Vec::new();
        for ch in text.chars() {
            match ch {
                '\n' => {
                    let line = std::mem::take(&mut self.current);
                    self.completed.push_back(line);
                    self.evict_into(&mut evicted);
                }
                '\r' => self.current.clear(),
                other => self.current.push(other),
            }
        }
        evicted
    }

    /// The lines currently visible in the tile, oldest first.
    ///
    /// This is the completed lines still in the window followed by the
    /// in-progress line, capped at the configured height.
    #[must_use]
    pub fn visible(&self) -> Vec<&str> {
        let mut lines: Vec<&str> = self.completed.iter().map(String::as_str).collect();
        lines.push(&self.current);
        lines
    }

    /// Consume the tail, returning every line still held (oldest first).
    ///
    /// Called when the stream ends: the visible completed lines plus any
    /// non-empty in-progress line, so the caller can flush the remainder to
    /// scrollback and leave a complete record.
    #[must_use]
    pub fn drain(self) -> Vec<String> {
        let mut lines: Vec<String> = self.completed.into();
        if !self.current.is_empty() {
            lines.push(self.current);
        }
        lines
    }

    /// Evict completed lines beyond the window; the in-progress line always
    /// occupies one slot, so the window holds at most `height - 1` completed
    /// lines.
    fn evict_into(&mut self, evicted: &mut Vec<String>) {
        while self.completed.len() >= self.height {
            if let Some(line) = self.completed.pop_front() {
                evicted.push(line);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::RegionTail;

    #[test]
    fn buffers_partial_line_until_newline() {
        let mut tail = RegionTail::new(3);
        assert!(tail.push_bytes(b"hel").is_empty());
        assert_eq!(tail.visible(), vec!["hel"]);
        assert!(tail.push_bytes(b"lo\n").is_empty());
        assert_eq!(tail.visible(), vec!["hello", ""]);
    }

    #[test]
    fn evicts_lines_past_the_window() {
        let mut tail = RegionTail::new(3);
        assert!(tail.push_bytes(b"a\n").is_empty());
        // With height 3 the window holds two completed lines plus the
        // in-progress line; the fourth completed line pushes "a" out.
        let evicted = tail.push_bytes(b"b\nc\nd\n");
        assert_eq!(evicted, vec!["a".to_string(), "b".to_string()]);
        assert_eq!(tail.visible(), vec!["c", "d", ""]);
    }

    #[test]
    fn carriage_return_resets_current_line() {
        let mut tail = RegionTail::new(3);
        tail.push_bytes(b"50%\r100%");
        assert_eq!(tail.visible(), vec!["100%"]);
    }

    #[test]
    fn drain_returns_all_remaining_lines() {
        let mut tail = RegionTail::new(5);
        tail.push_bytes(b"one\ntwo\nthree");
        assert_eq!(tail.drain(), vec!["one", "two", "three"]);
    }

    #[test]
    fn drain_omits_empty_trailing_line() {
        let mut tail = RegionTail::new(5);
        tail.push_bytes(b"one\n");
        assert_eq!(tail.drain(), vec!["one"]);
    }

    #[test]
    fn evicted_plus_remaining_reconstruct_full_stream() {
        let mut tail = RegionTail::new(2);
        let mut all = Vec::new();
        all.extend(tail.push_bytes(b"l1\nl2\nl3\nl4\n"));
        all.extend(tail.drain());
        assert_eq!(all, vec!["l1", "l2", "l3", "l4"]);
    }

    #[test]
    fn lossy_decoding_survives_invalid_utf8() {
        let mut tail = RegionTail::new(2);
        tail.push_bytes(&[0xff, b'o', b'k']);
        assert_eq!(tail.visible().len(), 1);
        assert!(tail.visible()[0].ends_with("ok"));
    }
}
