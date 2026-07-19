//! One-off status feedback lines for guided, multi-step CLI flows.
//!
//! Where [`crate::progress`] animates *ongoing* work and [`crate::prompt`] reads *input*,
//! this renders the short, one-shot status lines a flow emits between steps: `✓ Detected Rust`,
//! a `[1/4]` step counter, a section heading, a warn or error notice. It composes the two theme layers
//! — a [`Palette`] for color and a [`Glyphs`] set for the leading symbol —
//! so every line honours `NO_COLOR`, TTY detection,
//! and UTF-8 capability exactly like the rest of the CLI surface.
//!
//! The writer is injected,
//! so callers bind it to a real stream with [`StatusReporter::from_env`] (stderr, matching the "diagnostics to stderr" convention) while tests assert on an in-memory buffer via [`StatusReporter::new`].

use std::io::{self, Write};

use rskit_errors::{AppError, AppResult};

use crate::theme::{ColorChoice, Glyphs, Palette};

/// A status-line reporter over an injected writer, palette, and glyph set.
pub struct StatusReporter<W> {
    writer: W,
    palette: Palette,
    glyphs: Glyphs,
}

impl StatusReporter<io::Stderr> {
    /// Build a reporter bound to process stderr.
    ///
    /// The [`Palette`] resolves from `color` against stderr and the [`Glyphs`] from the process locale,
    /// so color and symbols both honour redirection, `NO_COLOR`, and terminal encoding.
    #[must_use]
    pub fn from_env(color: ColorChoice) -> Self {
        let stderr = io::stderr();
        let palette = Palette::for_stream(color, &stderr);
        Self {
            writer: stderr,
            palette,
            glyphs: Glyphs::from_env(),
        }
    }
}

impl<W: Write> StatusReporter<W> {
    /// Build a reporter from explicit parts.
    #[must_use]
    pub const fn new(writer: W, palette: Palette, glyphs: Glyphs) -> Self {
        Self {
            writer,
            palette,
            glyphs,
        }
    }

    /// Emit a success line: a green check glyph followed by `message`.
    ///
    /// # Errors
    ///
    /// Returns an error when the underlying writer fails.
    pub fn success(&mut self, message: &str) -> AppResult<()> {
        let prefix = self.palette.success(self.glyphs.success()).into_owned();
        self.write_line(&prefix, message)
    }

    /// Emit an error line: a red cross glyph followed by `message`.
    ///
    /// # Errors
    ///
    /// Returns an error when the underlying writer fails.
    pub fn error(&mut self, message: &str) -> AppResult<()> {
        let prefix = self.palette.error(self.glyphs.error()).into_owned();
        self.write_line(&prefix, message)
    }

    /// Emit a warning line: a yellow warning glyph followed by `message`.
    ///
    /// # Errors
    ///
    /// Returns an error when the underlying writer fails.
    pub fn warn(&mut self, message: &str) -> AppResult<()> {
        let prefix = self.palette.warn(self.glyphs.warning()).into_owned();
        self.write_line(&prefix, message)
    }

    /// Emit an informational line: a cyan info glyph followed by `message`.
    ///
    /// # Errors
    ///
    /// Returns an error when the underlying writer fails.
    pub fn info(&mut self, message: &str) -> AppResult<()> {
        let prefix = self.palette.info(self.glyphs.info()).into_owned();
        self.write_line(&prefix, message)
    }

    /// Emit an indented bullet line: a dimmed bullet glyph followed by `message`.
    ///
    /// # Errors
    ///
    /// Returns an error when the underlying writer fails.
    pub fn bullet(&mut self, message: &str) -> AppResult<()> {
        let prefix = format!("  {}", self.palette.dim(self.glyphs.bullet()));
        self.write_line(&prefix, message)
    }

    /// Emit a step line prefixed with a dimmed `[current/total]` counter.
    ///
    /// # Errors
    ///
    /// Returns an error when the underlying writer fails.
    pub fn step(&mut self, current: usize, total: usize, message: &str) -> AppResult<()> {
        let counter = self
            .palette
            .dim(&format!("[{current}/{total}]"))
            .into_owned();
        self.write_line(&counter, message)
    }

    /// Emit a bold section heading preceded by a blank line.
    ///
    /// # Errors
    ///
    /// Returns an error when the underlying writer fails.
    pub fn heading(&mut self, title: &str) -> AppResult<()> {
        let styled = self.palette.bold(title).into_owned();
        writeln!(self.writer, "\n{styled}").map_err(AppError::internal)
    }

    fn write_line(&mut self, prefix: &str, message: &str) -> AppResult<()> {
        writeln!(self.writer, "{prefix} {message}").map_err(AppError::internal)
    }
}

#[cfg(test)]
mod tests {
    use super::StatusReporter;
    use crate::theme::{Glyphs, Palette};

    fn reporter(buffer: &mut Vec<u8>, color: bool, unicode: bool) -> StatusReporter<&mut Vec<u8>> {
        StatusReporter::new(buffer, Palette::new(color), Glyphs::new(unicode))
    }

    fn rendered(buffer: Vec<u8>) -> String {
        String::from_utf8(buffer).expect("utf8")
    }

    #[test]
    fn success_line_carries_glyph_and_message() {
        let mut buffer = Vec::new();
        reporter(&mut buffer, false, true)
            .success("Detected Rust")
            .expect("write");
        let out = rendered(buffer);
        assert!(out.contains('✓'));
        assert!(out.contains("Detected Rust"));
    }

    #[test]
    fn step_line_renders_counter() {
        let mut buffer = Vec::new();
        reporter(&mut buffer, false, true)
            .step(1, 4, "Selecting ecosystems")
            .expect("write");
        let out = rendered(buffer);
        assert!(out.contains("[1/4]"));
        assert!(out.contains("Selecting ecosystems"));
    }

    #[test]
    fn heading_is_preceded_by_blank_line() {
        let mut buffer = Vec::new();
        reporter(&mut buffer, false, true)
            .heading("Configuration")
            .expect("write");
        let out = rendered(buffer);
        assert!(out.starts_with('\n'));
        assert!(out.contains("Configuration"));
    }

    #[test]
    fn ascii_fallback_avoids_unicode_glyphs() {
        let mut buffer = Vec::new();
        reporter(&mut buffer, false, false)
            .warn("no toolchain found")
            .expect("write");
        let out = rendered(buffer);
        assert!(out.contains('!'));
        assert!(!out.contains('⚠'));
    }

    #[test]
    fn disabled_palette_is_byte_clean() {
        let mut buffer = Vec::new();
        reporter(&mut buffer, false, true)
            .info("nothing to do")
            .expect("write");
        let out = rendered(buffer);
        assert!(!out.contains('\u{1b}'), "no color must be byte-clean");
    }

    #[test]
    fn enabled_palette_emits_ansi_escapes() {
        let mut buffer = Vec::new();
        reporter(&mut buffer, true, true)
            .error("build failed")
            .expect("write");
        let out = rendered(buffer);
        assert!(out.contains('\u{1b}'), "color must emit SGR escapes");
    }

    #[test]
    fn bullet_line_is_indented_with_its_glyph() {
        let mut buffer = Vec::new();
        reporter(&mut buffer, false, true)
            .bullet("cached crate")
            .expect("write");
        let out = rendered(buffer);
        assert!(out.starts_with("  "));
        assert!(out.contains('•'));
        assert!(out.contains("cached crate"));
    }

    #[test]
    fn from_env_binds_to_stderr_without_writing() {
        // Constructing over the process stderr must succeed regardless of TTY or NO_COLOR state;
        // it resolves the palette and glyphs but emits nothing.
        let _reporter = StatusReporter::from_env(crate::theme::ColorChoice::Never);
    }
}
