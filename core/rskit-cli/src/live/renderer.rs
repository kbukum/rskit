//! [`LiveConsole`] — a multi-region live terminal renderer.
//!
//! Renders several concurrent output streams as fixed-height "tiles" stacked in
//! a live area at the bottom of the terminal, each showing a bounded virtual
//! terminal of one stream (via [`RegionScreen`]). The tiles are an ephemeral
//! progress peek: rows that scroll off the top are dropped from the live view,
//! not flushed to scrollback, so a chatty stream does not flood the transcript.
//! Durable signal is emitted only at [`finish`](LiveConsole::finish) time — a
//! one-line verdict — and, for a failed region, a bounded replay of its retained
//! tail via [`finish_with_replay`](LiveConsole::finish_with_replay).
//!
//! The terminal mechanics (cursor positioning, redraw rate-limiting, width
//! handling, resize) are delegated to `indicatif`'s multi-progress engine, so
//! this type only owns the tile layout, the bounded failure ring, and the
//! verdict flush. It is generic: callers feed labeled byte streams and get
//! terminal rendering out, with no domain types involved.

use std::collections::HashMap;
use std::collections::VecDeque;

use indicatif::{MultiProgress, ProgressBar, ProgressDrawTarget, ProgressStyle};

use super::screen::RegionScreen;

/// The string each tile indents its content by, beneath the region header. The
/// virtual terminal grid is sized to the tile width minus this indent's width
/// so a child's output fills the visible content area exactly, without being
/// chopped.
const TILE_INDENT: &str = "  ";

/// How the live console lays out and truncates tiles.
///
/// The console does not auto-detect the terminal width or react to resizes:
/// `cols` is a fixed tile width the caller supplies once. Passing a value that
/// does not match the real terminal only over- or under-truncates the tiles; it
/// never corrupts output, since every tile line is clamped to `cols`.
#[derive(Debug, Clone, Copy)]
pub struct LiveConfig {
    /// Maximum content rows a region tile may occupy — the height of its virtual
    /// terminal and the cap a tile grows to. A tile does not reserve this height:
    /// it starts at just its header and grows with its content up to this many
    /// rows (see [`LiveConsole::feed`]), so a silent or short-lived region stays
    /// small instead of padding out a tall empty block.
    pub rows: usize,
    /// Terminal columns: the tile width. A child's output is applied to a grid
    /// sized to the visible content area (this width minus the content indent),
    /// so a real width must be passed (unlike the old truncation width, `0` is
    /// not "disable").
    pub cols: usize,
    /// How many rows that scroll off a tile are retained per region for a
    /// failure replay. The live tile stays a bounded peek; on failure this many
    /// of the most-recent scrolled-off rows (plus the final on-screen rows) are
    /// flushed to scrollback as the failure block. `0` retains nothing, so a
    /// failure replays only the rows still on screen.
    pub scrollback: usize,
}

impl Default for LiveConfig {
    fn default() -> Self {
        Self {
            rows: 5,
            cols: 80,
            scrollback: 200,
        }
    }
}

impl LiveConfig {
    /// The inner virtual-terminal grid width: the tile width minus the content
    /// indent rendered under each region header.
    ///
    /// A child whose output is fed to the tile must be told its terminal is this
    /// wide (not the full tile width), so its own line wrapping matches the grid.
    /// Otherwise a full-width in-place progress redraw wraps at the grid edge,
    /// scrolls the short grid, and churns the retained failure tail with stale
    /// half-frames on every tick — which then surface in the bounded replay when
    /// a region fails.
    #[must_use]
    pub fn content_cols(&self) -> usize {
        self.cols.saturating_sub(TILE_INDENT.len()).max(1)
    }
}

/// One in-flight region: its virtual terminal, label, backing progress line,
/// and a bounded ring of the rows that have scrolled off its tile — kept only
/// so a failure can replay recent context.
struct Region {
    bar: ProgressBar,
    screen: RegionScreen,
    label: String,
    retained: VecDeque<String>,
    /// The tallest content the tile has shown so far, capped at the grid height.
    /// The tile is rendered to this many rows so it grows with output but never
    /// shrinks mid-run — output that clears (an in-place progress redraw) leaves
    /// the tile at its high-water height rather than collapsing and reflowing.
    high_water: usize,
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
    /// Children feeding a tile should be sized to this; see
    /// [`LiveConfig::content_cols`].
    #[must_use]
    pub fn content_cols(&self) -> usize {
        self.config.content_cols()
    }

    /// Set the status line shown above the tiles.
    pub fn set_header(&self, text: impl Into<String>) {
        self.header.set_message(text.into());
    }

    /// Start a new region tile labeled `label`, keyed by `id`.
    ///
    /// A duplicate `id` discards the existing region first — dropping its tile
    /// and retained tail — so a re-used key neither leaks a stale tile nor
    /// double-reports.
    pub fn begin(&mut self, id: impl Into<String>, label: impl Into<String>) {
        let id = id.into();
        let label = label.into();
        if let Some(old) = self.regions.remove(&id) {
            old.bar.finish_and_clear();
            self.multi.remove(&old.bar);
        }
        let bar = self.multi.add(ProgressBar::new(0));
        bar.set_style(message_style());
        let region = Region {
            bar,
            screen: RegionScreen::new(self.config.rows, self.content_cols()),
            label,
            retained: VecDeque::new(),
            high_water: 0,
        };
        // A fresh region shows only its header: it grows to fit content as bytes
        // arrive, so a silent or instantly-finishing unit never paints a tall
        // block of blank rows.
        region.bar.set_message(render_tile(
            &region.label,
            &[] as &[&str],
            self.config.cols,
            0,
        ));
        self.regions.insert(id, region);
    }

    /// Feed raw output bytes to region `id`, updating its tile.
    ///
    /// Rows evicted from the tile are dropped from the live view but appended to
    /// the region's bounded retention ring, so a later failure can replay recent
    /// context. A feed for an unknown `id` is ignored, so late output after a
    /// finish cannot panic.
    pub fn feed(&mut self, id: &str, bytes: &[u8]) {
        let scrollback = self.config.scrollback;
        let Some(region) = self.regions.get_mut(id) else {
            return;
        };
        for line in region.screen.feed(bytes) {
            region.retained.push_back(line);
            while region.retained.len() > scrollback {
                region.retained.pop_front();
            }
        }
        let visible = region.screen.render();
        // Grow the tile to the tallest content it has shown, capped at the grid
        // height, and render exactly that many rows. `high_water` only rises, so
        // an in-place redraw that momentarily clears rows does not shrink the
        // tile and reflow the live area.
        let filled = content_height(&visible);
        region.high_water = region.high_water.max(filled).min(self.config.rows);
        region.bar.set_message(render_tile(
            &region.label,
            &visible,
            self.config.cols,
            region.high_water,
        ));
    }

    /// Finish region `id`, removing its tile and printing `verdict` as the only
    /// scrollback line — the collapsed signal for a succeeding unit.
    ///
    /// The region's peeked output is discarded, not flushed: on success the
    /// verdict (which the caller may enrich with a run summary) is the whole
    /// story. A finish for an unknown `id` prints only the verdict. Returns any
    /// I/O error from the verdict write.
    pub fn finish(&mut self, id: &str, verdict: impl AsRef<str>) -> std::io::Result<()> {
        if let Some(region) = self.regions.remove(id) {
            region.bar.finish_and_clear();
            self.multi.remove(&region.bar);
        }
        self.multi.println(verdict.as_ref())
    }

    /// Finish a failed region `id`, replaying its retained tail to scrollback as
    /// one contiguous, label-prefixed block, then printing `verdict`.
    ///
    /// The replay is the region's retention ring (rows that scrolled off the
    /// tile, oldest first) followed by the rows still on screen — a bounded,
    /// un-interleaved failure transcript. A finish for an unknown `id` prints
    /// only the verdict. Returns any I/O error from the flush or verdict write.
    pub fn finish_with_replay(
        &mut self,
        id: &str,
        verdict: impl AsRef<str>,
    ) -> std::io::Result<()> {
        if let Some(region) = self.regions.remove(id) {
            let Region {
                bar,
                screen,
                label,
                retained,
                high_water: _,
            } = region;
            for line in replay_body(&retained, screen.drain()) {
                let prefixed = scrollback_line(&label, &line);
                self.multi.println(truncate(&prefixed, self.config.cols))?;
            }
            bar.finish_and_clear();
            self.multi.remove(&bar);
        }
        self.multi.println(verdict.as_ref())
    }

    /// Print `line` to scrollback above the live area, without touching any tile.
    ///
    /// For output that is not tied to a live region — a header banner, or a
    /// completed unit's buffered block on the rare path where a unit is not
    /// live-tailed. Returns any I/O error from the write.
    pub fn note(&self, line: impl AsRef<str>) -> std::io::Result<()> {
        self.multi.println(line.as_ref())
    }

    /// Drop every remaining region, then clear the live area and blank the
    /// header, leaving the console reusable.
    ///
    /// Remaining regions are units that never finished; their peeked output is
    /// discarded (durable signal is emitted at finish time). Returns any I/O
    /// error from the terminal clear.
    pub fn clear(&mut self) -> std::io::Result<()> {
        for (_, region) in self.regions.drain() {
            region.bar.finish_and_clear();
            self.multi.remove(&region.bar);
        }
        self.header.set_message("");
        self.multi.clear()
    }

    /// The rows a region has retained for a failure replay (oldest first).
    #[cfg(test)]
    fn retained(&self, id: &str) -> Option<Vec<String>> {
        self.regions
            .get(id)
            .map(|region| region.retained.iter().cloned().collect())
    }

    /// The rendered height of a region's tile in lines: its header plus the
    /// current high-water content rows.
    #[cfg(test)]
    fn tile_height(&self, id: &str) -> Option<usize> {
        self.regions.get(id).map(|region| region.high_water + 1)
    }
}

/// A bar style that renders only its message (no bar, timer, or spinner).
fn message_style() -> ProgressStyle {
    ProgressStyle::with_template("{msg}").unwrap_or_else(|_| ProgressStyle::default_spinner())
}

/// The number of rows up to and including the last non-blank one — the height a
/// tile must render to show all its current content. Trailing blank grid rows
/// are excluded so a tile grows only to the content it actually holds.
fn content_height<S: AsRef<str>>(lines: &[S]) -> usize {
    lines
        .iter()
        .rposition(|line| !line.as_ref().is_empty())
        .map_or(0, |index| index + 1)
}

/// Render one tile as a labeled header line plus exactly `rows` indented
/// content lines (padded with blanks), each truncated to `cols` display
/// columns. As with [`truncate`], `cols == 0` disables truncation (used only by
/// tests; the live console always resolves `cols` to at least 1). `rows` is the
/// caller's chosen tile height (its content high-water mark), so the tile is
/// exactly as tall as the content it currently holds rather than a fixed block.
fn render_tile<S: AsRef<str>>(label: &str, lines: &[S], cols: usize, rows: usize) -> String {
    let header = format!("{}", console::style(format!("• {label}")).bold());
    let mut out = truncate(&header, cols);
    for index in 0..rows {
        out.push('\n');
        let line = lines.get(index).map_or("", AsRef::as_ref);
        out.push_str(&truncate(&format!("{TILE_INDENT}{line}"), cols));
    }
    out
}

/// Prefix a scrolled-out line with its region label for scrollback attribution.
fn scrollback_line(label: &str, line: &str) -> String {
    format!("{} {line}", console::style(format!("{label} │")).dim())
}

/// Build a failed region's replay body: its retained scrolled-off rows (oldest
/// first) followed by the rows still on screen, with trailing blank rows
/// trimmed. The caller label-prefixes each line for scrollback attribution.
fn replay_body(retained: &VecDeque<String>, on_screen: Vec<String>) -> Vec<String> {
    let mut lines: Vec<String> = retained.iter().cloned().collect();
    lines.extend(on_screen);
    while lines.last().is_some_and(String::is_empty) {
        lines.pop();
    }
    lines
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
    use std::collections::VecDeque;

    use super::{LiveConfig, LiveConsole, render_tile, replay_body, scrollback_line, truncate};

    fn config(rows: usize, cols: usize) -> LiveConfig {
        LiveConfig {
            rows,
            cols,
            ..LiveConfig::default()
        }
    }

    #[test]
    fn content_cols_reserves_the_indent_and_never_underflows() {
        assert_eq!(config(6, 80).content_cols(), 78);
        // Degenerate widths clamp to a usable grid rather than underflowing.
        assert_eq!(config(6, 1).content_cols(), 1);
        assert_eq!(config(6, 0).content_cols(), 1);
    }

    #[test]
    fn drives_full_region_lifecycle_without_panicking() -> std::io::Result<()> {
        let mut console = LiveConsole::hidden(config(2, 40));
        console.set_header("wave 1/2 · running 1");
        console.begin("u1", "rust:core#test");
        console.feed("u1", b"compiling\r\n");
        console.feed("u1", b"running 3 tests\r\nok\r\nok\r\n");
        console.finish("u1", "ok rust:core#test")?;
        console.clear()
    }

    #[test]
    fn reused_id_replaces_region_without_leaking() -> std::io::Result<()> {
        let mut console = LiveConsole::hidden(LiveConfig::default());
        console.begin("u1", "first");
        console.feed("u1", b"old\n");
        console.begin("u1", "second");
        console.feed("u1", b"new\n");
        console.finish("u1", "ok")
    }

    #[test]
    fn console_is_reusable_after_clear() -> std::io::Result<()> {
        let mut console = LiveConsole::hidden(LiveConfig::default());
        console.set_header("first pass");
        console.begin("u1", "task");
        console.feed("u1", b"partial output\n");
        console.clear()?;
        console.set_header("second pass");
        console.begin("u1", "task");
        console.feed("u1", b"more\n");
        console.finish("u1", "ok")
    }

    #[test]
    fn zero_rows_still_renders_content() -> std::io::Result<()> {
        let mut console = LiveConsole::hidden(config(0, 40));
        console.begin("u1", "task");
        console.feed("u1", b"visible line\r\n");
        console.finish("u1", "ok")
    }

    #[test]
    fn feed_and_finish_for_unknown_region_are_ignored() -> std::io::Result<()> {
        let mut console = LiveConsole::hidden(LiveConfig::default());
        console.feed("ghost", b"noise\n");
        console.finish("ghost", "done")
    }

    #[test]
    fn note_prints_to_scrollback_without_a_region() -> std::io::Result<()> {
        let console = LiveConsole::hidden(LiveConfig::default());
        console.note("standalone line")
    }

    #[test]
    fn scrolled_rows_are_retained_for_replay_not_lost() {
        let mut console = LiveConsole::hidden(LiveConfig {
            rows: 2,
            cols: 20,
            scrollback: 200,
        });
        console.begin("u1", "task");
        console.feed("u1", b"l1\nl2\nl3\nl4\n");
        // Only the last row stays on screen; the earlier rows scrolled off but
        // are retained for a possible failure replay.
        assert_eq!(
            console.retained("u1"),
            Some(vec!["l1".into(), "l2".into(), "l3".into()])
        );
    }

    #[test]
    fn retention_ring_is_bounded_under_a_long_feed() {
        let mut console = LiveConsole::hidden(LiveConfig {
            rows: 2,
            cols: 20,
            scrollback: 3,
        });
        console.begin("u1", "task");
        for _ in 0..50 {
            console.feed("u1", b"line\n");
        }
        assert!(console.retained("u1").is_some_and(|rows| rows.len() <= 3));
    }

    #[test]
    fn replay_body_orders_retained_then_on_screen_and_trims_blanks() {
        let retained = VecDeque::from(vec!["a".to_string(), "b".to_string()]);
        let on_screen = vec!["c".to_string(), "d".to_string(), String::new()];
        assert_eq!(replay_body(&retained, on_screen), vec!["a", "b", "c", "d"]);
    }

    #[test]
    fn failure_replay_and_success_finish_both_drop_the_region() -> std::io::Result<()> {
        let mut console = LiveConsole::hidden(config(2, 20));
        console.begin("ok", "task");
        console.feed("ok", b"noise\n");
        console.finish("ok", "ok task")?;
        assert!(console.retained("ok").is_none());

        console.begin("bad", "task");
        console.feed("bad", b"panic!\n");
        console.finish_with_replay("bad", "failed task")?;
        assert!(console.retained("bad").is_none());
        Ok(())
    }

    #[test]
    fn render_tile_labels_and_indents_lines() {
        let tile = render_tile("core", &["a", "b"], 0, 2);
        let stripped = console::strip_ansi_codes(&tile);
        assert_eq!(stripped, "• core\n  a\n  b");
    }

    #[test]
    fn content_height_counts_up_to_the_last_non_blank_row() {
        assert_eq!(super::content_height::<&str>(&[]), 0);
        assert_eq!(super::content_height(&["", "", ""]), 0);
        assert_eq!(super::content_height(&["a", "", ""]), 1);
        assert_eq!(super::content_height(&["a", "", "b", ""]), 3);
    }

    #[test]
    fn a_silent_region_renders_only_its_header() {
        let mut console = LiveConsole::hidden(config(12, 40));
        console.begin("u1", "rust:core#test");
        // Nothing fed yet: the tile is a single header line, not a block of
        // reserved blank rows.
        assert_eq!(console.tile_height("u1"), Some(1));
    }

    #[test]
    fn a_tile_grows_with_content_up_to_the_cap() {
        let mut console = LiveConsole::hidden(config(3, 40));
        console.begin("u1", "task");
        console.feed("u1", b"one\n");
        // header + one content row.
        assert_eq!(console.tile_height("u1"), Some(2));
        console.feed("u1", b"two\nthree\nfour\nfive");
        // Capped at the grid height (3 content rows) + header.
        assert_eq!(console.tile_height("u1"), Some(4));
    }

    #[test]
    fn a_grown_tile_does_not_shrink_when_output_clears() {
        let mut console = LiveConsole::hidden(config(3, 40));
        console.begin("u1", "task");
        console.feed("u1", b"a\r\nb\r\nc");
        assert_eq!(console.tile_height("u1"), Some(4));
        // An in-place redraw that collapses to a single line keeps the tile at
        // its high-water height instead of reflowing the live area.
        console.feed("u1", b"\x1b[H\x1b[2Jshort");
        assert_eq!(console.tile_height("u1"), Some(4));
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
    fn replay_line_clamped_to_width_never_wraps() {
        // A content row is sized to the tile's content area, but the scrollback
        // replay prefixes it with the full unit label — the composed line must be
        // clamped to the console width so it never wraps into an orphan fragment.
        let content = "x".repeat(200);
        let composed = scrollback_line("rust@rust:core#test", &content);
        let clamped = truncate(&composed, 140);
        assert!(console::measure_text_width(&clamped) <= 140);
    }

    #[test]
    fn truncate_respects_width_and_passthrough() {
        assert_eq!(truncate("hello world", 0), "hello world");
        let cut = truncate("hello world", 5);
        assert!(console::measure_text_width(&cut) <= 5);
    }
}
