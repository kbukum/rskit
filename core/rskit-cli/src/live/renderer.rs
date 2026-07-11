//! [`LiveConsole`] — a multi-region live terminal renderer.
//!
//! Renders several concurrent output streams as fixed-height "tiles" stacked in
//! a live area at the bottom of the terminal, each showing a bounded virtual
//! terminal of one stream (via [`RegionScreen`]). Rows that scroll out of a tile
//! — and a region's remaining rows when it finishes — are flushed into normal
//! scrollback above the live area, so the transcript stays complete while the
//! live area keeps a bounded height.
//!
//! The terminal mechanics (cursor positioning, redraw rate-limiting, width
//! handling, resize) are delegated to `indicatif`'s multi-progress engine, so
//! this type only owns the tile layout and the scrollback flush. It is generic:
//! callers feed labeled byte streams and get terminal rendering out, with no
//! domain types involved.

use std::collections::HashMap;

use indicatif::{MultiProgress, ProgressBar, ProgressDrawTarget, ProgressStyle};

use super::screen::RegionScreen;

/// Columns each tile indents its content beneath the region header. The virtual
/// terminal grid is sized to the tile width minus this indent so a child's
/// output fills the visible content area exactly, without being chopped.
const TILE_INDENT: usize = 2;

/// How the live console lays out and truncates tiles.
///
/// The console does not auto-detect the terminal width or react to resizes:
/// `cols` is a fixed tile width the caller supplies once. Passing a value that
/// does not match the real terminal only over- or under-truncates the tiles; it
/// never corrupts output, since every tile line is clamped to `cols`.
#[derive(Debug, Clone, Copy)]
pub struct LiveConfig {
    /// Content rows shown per region tile — the height of its virtual terminal.
    pub rows: usize,
    /// Terminal columns: the tile width. A child's output is applied to a grid
    /// sized to the visible content area (this width minus the content indent),
    /// so a real width must be passed (unlike the old truncation width, `0` is
    /// not "disable").
    pub cols: usize,
}

impl Default for LiveConfig {
    fn default() -> Self {
        Self { rows: 5, cols: 80 }
    }
}

/// One in-flight region: its virtual terminal, label, and backing progress line.
struct Region {
    bar: ProgressBar,
    screen: RegionScreen,
    label: String,
}

/// A live, multi-region terminal renderer.
///
/// Lifecycle per region: [`begin`](Self::begin) with a label,
/// [`feed`](Self::feed) raw bytes as they arrive, then [`finish`](Self::finish)
/// with a one-line verdict. [`set_header`](Self::set_header) updates a status
/// line pinned above the tiles.
pub struct LiveConsole {
    multi: MultiProgress,
    header: ProgressBar,
    regions: HashMap<String, Region>,
    config: LiveConfig,
}

impl LiveConsole {
    /// Create a console that renders to stderr.
    ///
    /// stderr keeps the live UI off stdout, leaving any machine-readable stream
    /// there uncorrupted.
    #[must_use]
    pub fn to_stderr(config: LiveConfig) -> Self {
        Self::with_target(ProgressDrawTarget::stderr(), config)
    }

    /// Create a console whose output is discarded — for tests and non-terminal
    /// runs where the live area must not render.
    #[must_use]
    pub fn hidden(config: LiveConfig) -> Self {
        Self::with_target(ProgressDrawTarget::hidden(), config)
    }

    fn with_target(target: ProgressDrawTarget, mut config: LiveConfig) -> Self {
        // Keep the grid and the renderer consistent: a tile always shows at
        // least one content row and its virtual terminal needs a real width.
        config.rows = config.rows.max(1);
        config.cols = config.cols.max(1);
        let multi = MultiProgress::with_draw_target(target);
        let header = multi.add(ProgressBar::new(0));
        header.set_style(message_style());
        Self {
            multi,
            header,
            regions: HashMap::new(),
            config,
        }
    }

    /// The virtual terminal grid width: the tile width minus the content
    /// indent, so a child fills the visible content area without being chopped.
    fn content_cols(&self) -> usize {
        self.config.cols.saturating_sub(TILE_INDENT).max(1)
    }

    /// Set the status line shown above the tiles.
    pub fn set_header(&self, text: impl Into<String>) {
        self.header.set_message(text.into());
    }

    /// Start a new region tile labeled `label`, keyed by `id`.
    ///
    /// A duplicate `id` retires the existing region first — flushing its
    /// remaining lines to scrollback — so a re-used key neither leaks a stale
    /// tile nor drops its transcript.
    ///
    /// Returns any I/O error from flushing a replaced region to scrollback.
    pub fn begin(
        &mut self,
        id: impl Into<String>,
        label: impl Into<String>,
    ) -> std::io::Result<()> {
        let id = id.into();
        let label = label.into();
        if let Some(old) = self.regions.remove(&id) {
            self.retire(old)?;
        }
        let bar = self.multi.add(ProgressBar::new(0));
        bar.set_style(message_style());
        let region = Region {
            bar,
            screen: RegionScreen::new(self.config.rows, self.content_cols()),
            label,
        };
        region.bar.set_message(render_tile(
            &region.label,
            &[],
            self.config.cols,
            self.config.rows,
        ));
        self.regions.insert(id, region);
        Ok(())
    }

    /// Feed raw output bytes to region `id`, updating its tile.
    ///
    /// Rows evicted from the tile are flushed to scrollback immediately. A feed
    /// for an unknown `id` is ignored, so late output after a finish cannot
    /// panic. Returns any I/O error from the scrollback flush.
    pub fn feed(&mut self, id: &str, bytes: &[u8]) -> std::io::Result<()> {
        let Some(region) = self.regions.get_mut(id) else {
            return Ok(());
        };
        let evicted = region.screen.feed(bytes);
        for line in evicted {
            self.multi.println(scrollback_line(&region.label, &line))?;
        }
        let visible = region.screen.render();
        let visible: Vec<&str> = visible.iter().map(String::as_str).collect();
        region.bar.set_message(render_tile(
            &region.label,
            &visible,
            self.config.cols,
            self.config.rows,
        ));
        Ok(())
    }

    /// Finish region `id`, flushing its remaining lines to scrollback, removing
    /// the tile, and printing `verdict` as a final status line.
    ///
    /// A finish for an unknown `id` prints only the verdict. Returns any I/O
    /// error from the scrollback flush.
    pub fn finish(&mut self, id: &str, verdict: impl AsRef<str>) -> std::io::Result<()> {
        if let Some(region) = self.regions.remove(id) {
            self.retire(region)?;
        }
        self.multi.println(verdict.as_ref())
    }

    /// Flush a retired region's remaining rows to scrollback and drop its tile.
    fn retire(&self, region: Region) -> std::io::Result<()> {
        for line in region.screen.drain() {
            self.multi.println(scrollback_line(&region.label, &line))?;
        }
        region.bar.finish_and_clear();
        self.multi.remove(&region.bar);
        Ok(())
    }

    /// Print `line` to scrollback above the live area, without touching any tile.
    ///
    /// For output that is not tied to a live region — a header banner, or a
    /// completed unit's buffered block on the rare path where a unit is not
    /// live-tailed. Returns any I/O error from the write.
    pub fn note(&self, line: impl AsRef<str>) -> std::io::Result<()> {
        self.multi.println(line.as_ref())
    }

    /// Retire every remaining region — flushing each tail to scrollback — then
    /// clear the live area and blank the header, leaving the console reusable.
    ///
    /// Regions are retired in id order so the flushed scrollback is deterministic.
    /// Returns any I/O error from the flush or terminal clear.
    pub fn clear(&mut self) -> std::io::Result<()> {
        let mut regions: Vec<(String, Region)> = self.regions.drain().collect();
        regions.sort_by(|(a, _), (b, _)| a.cmp(b));
        for (_, region) in regions {
            self.retire(region)?;
        }
        self.header.set_message("");
        self.multi.clear()
    }
}

/// A bar style that renders only its message (no bar, timer, or spinner).
fn message_style() -> ProgressStyle {
    ProgressStyle::with_template("{msg}").unwrap_or_else(|_| ProgressStyle::default_spinner())
}

/// Render one tile as a labeled header line plus exactly `rows` indented
/// content lines (padded with blanks), each truncated to `cols` display
/// columns. The fixed line count keeps every tile a constant height so the live
/// area does not reflow as streams emit output.
fn render_tile(label: &str, lines: &[&str], cols: usize, rows: usize) -> String {
    let header = format!("{}", console::style(format!("• {label}")).bold());
    let mut out = truncate(&header, cols);
    let indent = " ".repeat(TILE_INDENT);
    for index in 0..rows {
        out.push('\n');
        let line = lines.get(index).copied().unwrap_or("");
        out.push_str(&truncate(&format!("{indent}{line}"), cols));
    }
    out
}

/// Prefix a scrolled-out line with its region label for scrollback attribution.
fn scrollback_line(label: &str, line: &str) -> String {
    format!("{} {line}", console::style(format!("{label} │")).dim())
}

/// Truncate `line` to `width` display columns (ANSI- and width-aware), leaving
/// it untouched when `width` is `0`.
fn truncate(line: &str, width: usize) -> String {
    if width == 0 {
        return line.to_string();
    }
    console::truncate_str(line, width, "…").into_owned()
}

#[cfg(test)]
mod tests {
    use super::{LiveConfig, LiveConsole, render_tile, scrollback_line, truncate};

    #[test]
    fn drives_full_region_lifecycle_without_panicking() -> std::io::Result<()> {
        let mut console = LiveConsole::hidden(LiveConfig { rows: 2, cols: 40 });
        console.set_header("wave 1/2 · running 1");
        console.begin("u1", "rust:core#test")?;
        console.feed("u1", b"compiling\r\n")?;
        console.feed("u1", b"running 3 tests\r\nok\r\nok\r\n")?;
        console.finish("u1", "ok rust:core#test")?;
        console.clear()
    }

    #[test]
    fn reused_id_replaces_region_without_leaking() -> std::io::Result<()> {
        let mut console = LiveConsole::hidden(LiveConfig::default());
        console.begin("u1", "first")?;
        console.feed("u1", b"old\n")?;
        console.begin("u1", "second")?;
        console.feed("u1", b"new\n")?;
        console.finish("u1", "ok")
    }

    #[test]
    fn console_is_reusable_after_clear() -> std::io::Result<()> {
        let mut console = LiveConsole::hidden(LiveConfig::default());
        console.set_header("first pass");
        console.begin("u1", "task")?;
        console.feed("u1", b"partial output\n")?;
        console.clear()?;
        console.set_header("second pass");
        console.begin("u1", "task")?;
        console.feed("u1", b"more\n")?;
        console.finish("u1", "ok")
    }

    #[test]
    fn zero_rows_still_renders_content() -> std::io::Result<()> {
        let mut console = LiveConsole::hidden(LiveConfig { rows: 0, cols: 40 });
        console.begin("u1", "task")?;
        console.feed("u1", b"visible line\r\n")?;
        console.finish("u1", "ok")
    }

    #[test]
    fn feed_and_finish_for_unknown_region_are_ignored() -> std::io::Result<()> {
        let mut console = LiveConsole::hidden(LiveConfig::default());
        console.feed("ghost", b"noise\n")?;
        console.finish("ghost", "done")
    }

    #[test]
    fn note_prints_to_scrollback_without_a_region() -> std::io::Result<()> {
        let console = LiveConsole::hidden(LiveConfig::default());
        console.note("standalone line")
    }

    #[test]
    fn render_tile_labels_and_indents_lines() {
        let tile = render_tile("core", &["a", "b"], 0, 2);
        let stripped = console::strip_ansi_codes(&tile);
        assert_eq!(stripped, "• core\n  a\n  b");
    }

    #[test]
    fn render_tile_pads_to_fixed_height() {
        let tile = render_tile("core", &["only"], 0, 3);
        let stripped = console::strip_ansi_codes(&tile);
        assert_eq!(stripped, "• core\n  only\n  \n  ");
    }

    #[test]
    fn render_tile_truncates_header_and_content_to_width() {
        let tile = render_tile("a-very-long-label", &["a-very-long-content-line"], 8, 1);
        for line in console::strip_ansi_codes(&tile).lines() {
            assert!(console::measure_text_width(line) <= 8);
        }
    }

    #[test]
    fn scrollback_line_prefixes_with_label() {
        let line = scrollback_line("core", "hello");
        let stripped = console::strip_ansi_codes(&line);
        assert_eq!(stripped, "core │ hello");
    }

    #[test]
    fn truncate_respects_width_and_passthrough() {
        assert_eq!(truncate("hello world", 0), "hello world");
        let cut = truncate("hello world", 5);
        assert!(console::measure_text_width(&cut) <= 5);
    }
}
