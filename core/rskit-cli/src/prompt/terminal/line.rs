//! Plain cooked-stdio terminal: the dependency-free, always-available default.

use std::io::{self, BufRead, Write};

use rskit_errors::{AppError, AppResult};

use super::{Capabilities, Terminal};
use crate::prompt::key::Key;

/// A line-driven [`Terminal`] over an injected reader and writer.
///
/// It uses the operating system's cooked line discipline: the user types a whole line
/// and presses Enter. It never enters raw mode, works over pipes, and needs no extra dependencies,
/// so it is the default terminal. Bind it to real streams with [`LineTerminal::stdio`],
/// or to in-memory buffers with [`LineTerminal::new`].
pub struct LineTerminal<R, W> {
    reader: R,
    writer: W,
}

impl LineTerminal<io::BufReader<io::Stdin>, io::Stderr> {
    /// Build a line terminal bound to process stdin and stderr.
    #[must_use]
    pub fn stdio() -> Self {
        Self::new(io::BufReader::new(io::stdin()), io::stderr())
    }
}

impl<R: BufRead, W: Write> LineTerminal<R, W> {
    /// Build a line terminal from an explicit reader and writer.
    #[must_use]
    pub const fn new(reader: R, writer: W) -> Self {
        Self { reader, writer }
    }
}

impl<R: BufRead, W: Write> Terminal for LineTerminal<R, W> {
    fn capabilities(&self) -> Capabilities {
        Capabilities::line_driven()
    }

    fn read_line(&mut self) -> AppResult<Option<String>> {
        let mut buffer = String::new();
        let read = self
            .reader
            .read_line(&mut buffer)
            .map_err(AppError::internal)?;
        if read == 0 {
            return Ok(None);
        }
        Ok(Some(buffer.trim_end_matches(['\n', '\r']).to_string()))
    }

    fn read_key(&mut self) -> AppResult<Key> {
        Err(AppError::invalid_input(
            "prompt",
            "line terminal does not support key input",
        ))
    }

    fn write(&mut self, text: &str) -> AppResult<()> {
        write!(self.writer, "{text}").map_err(AppError::internal)
    }

    fn write_line(&mut self, text: &str) -> AppResult<()> {
        writeln!(self.writer, "{text}").map_err(AppError::internal)
    }

    fn flush(&mut self) -> AppResult<()> {
        self.writer.flush().map_err(AppError::internal)
    }

    fn clear_last_lines(&mut self, _count: u16) -> AppResult<()> {
        Ok(())
    }

    fn begin_interactive(&mut self) -> AppResult<()> {
        Ok(())
    }

    fn end_interactive(&mut self) -> AppResult<()> {
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::{Capabilities, LineTerminal, Terminal};

    #[test]
    fn reads_trimmed_lines_then_eof() {
        let input = b"hello\nworld\n";
        let mut out = Vec::new();
        let mut term = LineTerminal::new(&input[..], &mut out);
        assert_eq!(term.capabilities(), Capabilities::line_driven());
        assert_eq!(term.read_line().expect("line"), Some("hello".to_string()));
        assert_eq!(term.read_line().expect("line"), Some("world".to_string()));
        assert_eq!(term.read_line().expect("eof"), None);
    }

    #[test]
    fn key_input_is_unsupported() {
        let mut out = Vec::new();
        let mut term = LineTerminal::new(&b""[..], &mut out);
        assert!(term.read_key().is_err());
    }

    #[test]
    fn writes_pass_through() {
        let mut out = Vec::new();
        {
            let mut term = LineTerminal::new(&b""[..], &mut out);
            term.write("a").expect("write");
            term.write_line("b").expect("writeln");
        }
        assert_eq!(String::from_utf8(out).expect("utf8"), "ab\n");
    }

    #[test]
    fn line_control_operations_are_no_ops_over_cooked_stdio() {
        let mut out = Vec::new();
        let mut term = LineTerminal::new(&b""[..], &mut out);
        term.flush().expect("flush");
        term.clear_last_lines(3).expect("clear is a no-op");
        term.begin_interactive().expect("begin is a no-op");
        term.end_interactive().expect("end is a no-op");
    }

    #[test]
    fn stdio_binds_to_process_streams() {
        // Constructing over the real process streams must succeed;
        // capabilities stay line-driven without touching stdin/stderr.
        let term = LineTerminal::stdio();
        assert_eq!(term.capabilities(), Capabilities::line_driven());
    }
}
