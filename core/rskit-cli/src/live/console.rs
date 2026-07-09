//! [`LiveConsole`] — a multi-region live terminal renderer.
//!
//! Renders several concurrent output streams as fixed-height "tiles" stacked in
//! a live area at the bottom of the terminal, each showing the last few lines of
//! one stream (via [`RegionTail`]). Lines that scroll out of a tile — and a
//! region's remaining lines when it finishes — are flushed into normal
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

use super::region::RegionTail;

/// How the live console lays out and truncates tiles.
#[derive(Debug, Clone, Copy)]
pub struct LiveConfig {
    /// Maximum content lines shown per region tile.
    pub tail_lines: usize,
    /// Terminal width used to truncate tile lines so a long line cannot wrap and
    /// break the fixed tile height. `0` disables truncation.
    pub width: usize,
}

impl Default for LiveConfig {
    fn default() -> Self {
        Self {
            tail_lines: 5,
            width: 0,
        }
    }
}

/// One in-flight region: its tail buffer, label, and backing progress line.
struct Region {
    bar: ProgressBar,
    tail: RegionTail,
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

    fn with_target(target: ProgressDrawTarget, config: LiveConfig) -> Self {
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

    /// Set the status line shown above the tiles.
    pub fn set_header(&self, text: impl Into<String>) {
        self.header.set_message(text.into());
    }

    /// Start a new region tile labeled `label`, keyed by `id`.
    ///
    /// A duplicate `id` retires the existing region first — flushing its
    /// remaining lines to scrollback — so a re-used key neither leaks a stale
    /// tile nor drops its transcript.
    pub fn begin(&mut self, id: impl Into<String>, label: impl Into<String>) {
        let id = id.into();
        let label = label.into();
        if let Some(old) = self.regions.remove(&id) {
            self.retire(old);
        }
        let bar = self.multi.add(ProgressBar::new(0));
        bar.set_style(message_style());
        let region = Region {
            bar,
            tail: RegionTail::new(self.config.tail_lines),
            label,
        };
        region
            .bar
            .set_message(render_tile(&region.label, &[], self.config.width));
        self.regions.insert(id, region);
    }

    /// Feed raw output bytes to region `id`, updating its tile.
    ///
    /// Lines evicted from the tile are flushed to scrollback immediately. A feed
    /// for an unknown `id` is ignored, so late output after a finish cannot
    /// panic.
    pub fn feed(&mut self, id: &str, bytes: &[u8]) {
        let Some(region) = self.regions.get_mut(id) else {
            return;
        };
        let evicted = region.tail.push_bytes(bytes);
        for line in evicted {
            self.multi
                .println(scrollback_line(&region.label, &line))
                .ok();
        }
        let visible = region.tail.visible();
        region
            .bar
            .set_message(render_tile(&region.label, &visible, self.config.width));
    }

    /// Finish region `id`, flushing its remaining lines to scrollback, removing
    /// the tile, and printing `verdict` as a final status line.
    ///
    /// A finish for an unknown `id` prints only the verdict.
    pub fn finish(&mut self, id: &str, verdict: impl AsRef<str>) {
        if let Some(region) = self.regions.remove(id) {
            self.retire(region);
        }
        self.multi.println(verdict.as_ref()).ok();
    }

    /// Flush a retired region's remaining tail to scrollback and drop its tile.
    fn retire(&self, region: Region) {
        for line in region.tail.drain() {
            self.multi
                .println(scrollback_line(&region.label, &line))
                .ok();
        }
        region.bar.finish_and_clear();
        self.multi.remove(&region.bar);
    }

    /// Print `line` to scrollback above the live area, without touching any tile.
    ///
    /// For output that is not tied to a live region — a header banner, or a
    /// completed unit's buffered block on the rare path where a unit is not
    /// live-tailed.
    pub fn note(&self, line: impl AsRef<str>) {
        self.multi.println(line.as_ref()).ok();
    }

    /// Clear the header and every remaining tile from the terminal.
    pub fn clear(&mut self) {
        for (_, region) in self.regions.drain() {
            region.bar.finish_and_clear();
            self.multi.remove(&region.bar);
        }
        self.header.finish_and_clear();
        self.multi.clear().ok();
    }
}

/// A bar style that renders only its message (no bar, timer, or spinner).
fn message_style() -> ProgressStyle {
    ProgressStyle::with_template("{msg}").unwrap_or_else(|_| ProgressStyle::default_spinner())
}

/// Render one tile as a labeled header line plus indented content lines, each
/// truncated to `width` display columns so no line can wrap and break the fixed
/// tile height.
fn render_tile(label: &str, lines: &[&str], width: usize) -> String {
    let header = format!("{}", console::style(format!("• {label}")).bold());
    let mut out = truncate(&header, width);
    for line in lines {
        out.push('\n');
        out.push_str(&truncate(&format!("  {line}"), width));
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
    fn drives_full_region_lifecycle_without_panicking() {
        let mut console = LiveConsole::hidden(LiveConfig {
            tail_lines: 2,
            width: 40,
        });
        console.set_header("wave 1/2 · running 1");
        console.begin("u1", "rust:core#test");
        console.feed("u1", b"compiling\n");
        console.feed("u1", b"running 3 tests\nok\nok\n");
        console.finish("u1", "ok rust:core#test");
        console.clear();
    }

    #[test]
    fn reused_id_replaces_region_without_leaking() {
        let mut console = LiveConsole::hidden(LiveConfig::default());
        console.begin("u1", "first");
        console.feed("u1", b"old\n");
        console.begin("u1", "second");
        console.feed("u1", b"new\n");
        console.finish("u1", "ok");
    }

    #[test]
    fn feed_and_finish_for_unknown_region_are_ignored() {
        let mut console = LiveConsole::hidden(LiveConfig::default());
        console.feed("ghost", b"noise\n");
        console.finish("ghost", "done");
    }

    #[test]
    fn note_prints_to_scrollback_without_a_region() {
        let console = LiveConsole::hidden(LiveConfig::default());
        console.note("standalone line");
    }

    #[test]
    fn render_tile_labels_and_indents_lines() {
        let tile = render_tile("core", &["a", "b"], 0);
        let stripped = console::strip_ansi_codes(&tile);
        assert_eq!(stripped, "• core\n  a\n  b");
    }

    #[test]
    fn render_tile_truncates_header_and_content_to_width() {
        let tile = render_tile("a-very-long-label", &["a-very-long-content-line"], 8);
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
