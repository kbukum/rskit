//! [`MakeWriter`] adapters that mask log lines before they reach the sink.

use std::io;
use std::sync::Arc;

use tracing_subscriber::fmt::MakeWriter;

use super::masker::Masker;

/// A [`MakeWriter`] wrapper that masks sensitive data in log output.
///
/// Wraps an inner writer
/// and applies masking via the supplied [`Masker`] to every log line before it reaches the underlying output.
///
/// # Examples
///
/// ```ignore
/// use rskit_logging::masking::{DefaultMasker, MaskingMakeWriter, Masker};
/// use std::sync::Arc;
///
/// let masker: Arc<dyn Masker> = Arc::new(DefaultMasker::default());
/// let writer = MaskingMakeWriter::new(std::io::stdout, masker);
/// ```
pub struct MaskingMakeWriter<W> {
    inner: W,
    masker: Arc<dyn Masker>,
}

impl<W> MaskingMakeWriter<W> {
    /// Create a new masking writer wrapper.
    ///
    /// `inner` is the underlying [`MakeWriter`] (e.g., `std::io::stdout`).
    /// `masker` is the masking engine wrapped in an [`Arc`].
    pub fn new(inner: W, masker: Arc<dyn Masker>) -> Self {
        Self { inner, masker }
    }
}

impl<'a, W: MakeWriter<'a>> MakeWriter<'a> for MaskingMakeWriter<W> {
    type Writer = MaskingWriter<W::Writer>;

    fn make_writer(&'a self) -> Self::Writer {
        MaskingWriter {
            inner: self.inner.make_writer(),
            masker: Arc::clone(&self.masker),
            buffer: Vec::with_capacity(256),
        }
    }

    fn make_writer_for(&'a self, meta: &tracing::Metadata<'_>) -> Self::Writer {
        MaskingWriter {
            inner: self.inner.make_writer_for(meta),
            masker: Arc::clone(&self.masker),
            buffer: Vec::with_capacity(256),
        }
    }
}

/// A writer that buffers output and applies masking on flush / drop.
///
/// Created by [`MaskingMakeWriter`]. Buffers all `write` calls
/// and applies masking when the writer is flushed or dropped (at the end of each log event).
pub struct MaskingWriter<W: io::Write> {
    inner: W,
    masker: Arc<dyn Masker>,
    buffer: Vec<u8>,
}

impl<W: io::Write> io::Write for MaskingWriter<W> {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        self.buffer.extend_from_slice(buf);
        Ok(buf.len())
    }

    fn flush(&mut self) -> io::Result<()> {
        if !self.buffer.is_empty() {
            let output = String::from_utf8_lossy(&self.buffer);
            let masked = self.masker.mask_output(&output);
            self.inner.write_all(masked.as_bytes())?;
            self.buffer.clear();
        }
        self.inner.flush()
    }
}

impl<W: io::Write> Drop for MaskingWriter<W> {
    fn drop(&mut self) {
        if !self.buffer.is_empty() {
            // Best-effort flush;
            // errors are silently ignored as is standard practice for log writers.
            let output = String::from_utf8_lossy(&self.buffer);
            let masked = self.masker.mask_output(&output);
            let _ = self.inner.write_all(masked.as_bytes());
            self.buffer.clear();
            let _ = self.inner.flush();
        }
    }
}
