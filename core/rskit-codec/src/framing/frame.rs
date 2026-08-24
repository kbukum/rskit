//! Raw byte-frame transport: a 4-byte big-endian length prefix plus payload.

use std::io::{Read, Write};

use rskit_errors::{AppError, AppResult, ErrorCode};

/// Default maximum accepted payload size for a single frame (16 MiB).
///
/// Generous enough for large structured payloads yet bounded
/// so a corrupt length prefix cannot trigger an unbounded allocation.
/// Callers may pass a tighter cap to [`read_frame`].
pub const DEFAULT_MAX_FRAME_BYTES: usize = 16 * 1024 * 1024;

/// Width of the big-endian length prefix that precedes every payload.
const LEN_PREFIX_BYTES: usize = 4;

/// Write one length-delimited frame carrying `payload`, flushing on completion.
///
/// # Errors
///
/// Returns a typed [`AppError`] if `payload` exceeds `max_bytes`
/// or the underlying writer fails (cause preserved).
pub fn write_frame<W: Write>(writer: &mut W, payload: &[u8], max_bytes: usize) -> AppResult<()> {
    if payload.len() > max_bytes {
        return Err(AppError::invalid_input(
            "frame",
            format!(
                "payload of {} bytes exceeds the {max_bytes}-byte frame limit",
                payload.len()
            ),
        ));
    }
    let len = u32::try_from(payload.len())
        .map_err(|_| AppError::invalid_input("frame", "payload length exceeds u32 range"))?;
    writer
        .write_all(&len.to_be_bytes())
        .map_err(|error| transport_error("write frame length", error))?;
    writer
        .write_all(payload)
        .map_err(|error| transport_error("write frame payload", error))?;
    writer
        .flush()
        .map_err(|error| transport_error("flush frame", error))?;
    Ok(())
}

/// Read one length-delimited frame, bounded by `max_bytes`.
///
/// Returns `Ok(None)` on a clean end-of-stream observed *before* any length byte (the peer closed the connection between frames).
/// A partial prefix or payload is a hard transport error.
///
/// # Errors
///
/// Returns a typed [`AppError`] on a truncated frame, a length above `max_bytes`,
/// or any underlying read failure (cause preserved).
pub fn read_frame<R: Read>(reader: &mut R, max_bytes: usize) -> AppResult<Option<Vec<u8>>> {
    let mut prefix = [0u8; LEN_PREFIX_BYTES];
    match read_exact_or_eof(reader, &mut prefix)? {
        ReadEnd::Eof => return Ok(None),
        ReadEnd::Filled => {}
    }
    let len = u32::from_be_bytes(prefix) as usize;
    if len > max_bytes {
        return Err(AppError::invalid_input(
            "frame",
            format!("incoming frame length {len} exceeds the {max_bytes}-byte limit"),
        ));
    }
    let mut payload = vec![0u8; len];
    reader
        .read_exact(&mut payload)
        .map_err(|error| transport_error("read frame payload", error))?;
    Ok(Some(payload))
}

/// Whether a fixed-size read filled the buffer or hit a clean EOF first.
enum ReadEnd {
    /// The buffer was completely filled.
    Filled,
    /// End-of-stream was reached before any byte was read.
    Eof,
}

/// Fill `buf` exactly, distinguishing a clean leading EOF from a truncated read.
fn read_exact_or_eof<R: Read>(reader: &mut R, buf: &mut [u8]) -> AppResult<ReadEnd> {
    let mut read = 0;
    while read < buf.len() {
        match reader.read(&mut buf[read..]) {
            Ok(0) => {
                if read == 0 {
                    return Ok(ReadEnd::Eof);
                }
                return Err(AppError::new(
                    ErrorCode::ServiceUnavailable,
                    "framed transport: stream ended mid-frame (truncated length prefix)",
                ));
            }
            Ok(count) => read += count,
            Err(error) if error.kind() == std::io::ErrorKind::Interrupted => {}
            Err(error) => return Err(transport_error("read frame length", error)),
        }
    }
    Ok(ReadEnd::Filled)
}

/// Build a typed transport error preserving the underlying I/O cause.
///
/// The original [`std::io::Error`] is attached by value so its OS error code
/// ([`std::io::Error::raw_os_error`]) and source chain survive on the returned
/// [`AppError`]; reconstructing a fresh error from `kind()`/`to_string()` would
/// silently drop both.
fn transport_error(context: &str, error: std::io::Error) -> AppError {
    AppError::new(
        ErrorCode::ServiceUnavailable,
        format!("framed transport: {context}"),
    )
    .with_cause(error)
}

#[cfg(test)]
mod tests {
    use std::io::{Read, Write};

    use super::{DEFAULT_MAX_FRAME_BYTES, read_frame, write_frame};

    #[test]
    fn round_trips_a_frame() {
        let mut buffer = Vec::new();
        write_frame(&mut buffer, b"hello", DEFAULT_MAX_FRAME_BYTES).expect("write");
        let mut cursor = std::io::Cursor::new(buffer);
        let frame = read_frame(&mut cursor, DEFAULT_MAX_FRAME_BYTES)
            .expect("read")
            .expect("frame present");
        assert_eq!(frame, b"hello");
    }

    #[test]
    fn clean_eof_between_frames_is_none() {
        let mut cursor = std::io::Cursor::new(Vec::new());
        assert!(
            read_frame(&mut cursor, DEFAULT_MAX_FRAME_BYTES)
                .expect("read")
                .is_none()
        );
    }

    #[test]
    fn truncated_prefix_is_a_transport_error() {
        let mut cursor = std::io::Cursor::new(vec![0u8, 0u8]);
        let error = read_frame(&mut cursor, DEFAULT_MAX_FRAME_BYTES).expect_err("truncated errors");
        assert_eq!(error.code(), rskit_errors::ErrorCode::ServiceUnavailable);
    }

    #[test]
    fn oversized_frame_is_rejected() {
        let mut buffer = Vec::new();
        write_frame(&mut buffer, &[0u8; 8], DEFAULT_MAX_FRAME_BYTES).expect("write");
        assert!(read_frame(&mut std::io::Cursor::new(buffer), 4).is_err());
    }

    #[test]
    fn write_frame_rejects_oversized_payload_and_preserves_io_errors() {
        struct FailingWriter;

        impl Write for FailingWriter {
            fn write(&mut self, _buf: &[u8]) -> std::io::Result<usize> {
                Err(std::io::Error::other("boom"))
            }

            fn flush(&mut self) -> std::io::Result<()> {
                Ok(())
            }
        }

        let mut buffer = Vec::new();
        assert!(write_frame(&mut buffer, b"too large", 1).is_err());
        assert!(write_frame(&mut FailingWriter, b"ok", DEFAULT_MAX_FRAME_BYTES).is_err());
    }

    #[test]
    fn transport_error_preserves_underlying_io_cause() {
        // ECONNRESET; a fresh io::Error rebuilt from kind()/to_string() would
        // report `raw_os_error() == None`, so this pins cause fidelity.
        const ECONNRESET: i32 = 104;

        struct OsErrorWriter;
        impl Write for OsErrorWriter {
            fn write(&mut self, _buf: &[u8]) -> std::io::Result<usize> {
                Err(std::io::Error::from_raw_os_error(ECONNRESET))
            }

            fn flush(&mut self) -> std::io::Result<()> {
                Ok(())
            }
        }

        let error = write_frame(&mut OsErrorWriter, b"payload", DEFAULT_MAX_FRAME_BYTES)
            .expect_err("writer failure must surface");
        let cause = error
            .cause()
            .expect("underlying I/O cause preserved")
            .downcast_ref::<std::io::Error>()
            .expect("cause is the original io::Error");
        assert_eq!(cause.raw_os_error(), Some(ECONNRESET));
    }

    #[test]
    fn read_frame_retries_interrupted_prefix_reads_and_reports_io_errors() {
        struct InterruptedThenData {
            data: std::io::Cursor<Vec<u8>>,
            interrupted: bool,
        }

        impl Read for InterruptedThenData {
            fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
                if !self.interrupted {
                    self.interrupted = true;
                    return Err(std::io::Error::from(std::io::ErrorKind::Interrupted));
                }
                self.data.read(buf)
            }
        }

        struct FailingReader;
        impl Read for FailingReader {
            fn read(&mut self, _buf: &mut [u8]) -> std::io::Result<usize> {
                Err(std::io::Error::other("boom"))
            }
        }

        let mut framed = Vec::new();
        write_frame(&mut framed, b"x", DEFAULT_MAX_FRAME_BYTES).unwrap();
        let mut reader = InterruptedThenData {
            data: std::io::Cursor::new(framed),
            interrupted: false,
        };
        assert_eq!(
            read_frame(&mut reader, DEFAULT_MAX_FRAME_BYTES)
                .unwrap()
                .unwrap(),
            b"x"
        );

        assert!(read_frame(&mut FailingReader, DEFAULT_MAX_FRAME_BYTES).is_err());
    }
}
