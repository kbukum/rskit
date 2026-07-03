use std::{
    io::{ErrorKind, Read, Write},
    process::{Child, ChildStdin},
    sync::{Arc, mpsc},
    thread,
};

use crate::capture::{BoundedOutput, SharedOutput, shared_output, take_shared};
use crate::{AppError, AppResult, ErrorCode};

use super::config::{PersistentConfig, PersistentOutputStream, PersistentReadiness};

pub(in crate::persistent) type Capture = SharedOutput;
pub(in crate::persistent) type CapturedOutput = BoundedOutput;
pub(in crate::persistent) type ReaderThread = Option<thread::JoinHandle<AppResult<()>>>;
pub(in crate::persistent) type StdinThread = Option<thread::JoinHandle<AppResult<()>>>;

pub(in crate::persistent) fn new_capture() -> Capture {
    shared_output()
}

pub(in crate::persistent) fn spawn_output_readers(
    child: &mut Child,
    stdout: &Capture,
    stderr: &Capture,
    ready_tx: &mpsc::Sender<()>,
    config: &PersistentConfig,
) -> (ReaderThread, ReaderThread) {
    let matcher = match &config.readiness {
        PersistentReadiness::OutputContains(value) => Some(value.clone()),
        PersistentReadiness::Started | PersistentReadiness::Command(_) => None,
    };
    let output = config.output;
    let observer = config.output_observer.clone();
    let stdout_thread = child.stdout.take().map(|reader| {
        spawn_reader(
            reader,
            Arc::clone(stdout),
            matcher.clone(),
            ready_tx.clone(),
            output.stdout_stream(),
            observer
                .clone()
                .and_then(|observer| observer.stdout_bytes_callback()),
            config.max_capture_bytes,
        )
    });
    let stderr_thread = child.stderr.take().map(|reader| {
        spawn_reader(
            reader,
            Arc::clone(stderr),
            matcher,
            ready_tx.clone(),
            output.stderr_stream(),
            observer.and_then(|observer| observer.stderr_bytes_callback()),
            config.max_capture_bytes,
        )
    });
    (stdout_thread, stderr_thread)
}

pub(in crate::persistent) fn spawn_stdin_writer(
    child: &mut Child,
    stdin: Option<Vec<u8>>,
) -> StdinThread {
    let bytes = stdin?;
    let stream = child.stdin.take()?;
    Some(thread::spawn(move || write_stdin(stream, bytes)))
}

pub(in crate::persistent) fn take_capture(capture: &Capture) -> CapturedOutput {
    take_shared(capture)
}

fn write_stdin(mut stream: ChildStdin, bytes: Vec<u8>) -> AppResult<()> {
    match stream.write_all(&bytes) {
        Ok(()) => Ok(()),
        Err(error) if error.kind() == ErrorKind::BrokenPipe => Ok(()),
        Err(error) => Err(AppError::new(
            ErrorCode::Internal,
            format!("failed to write to persistent process stdin: {error}"),
        )),
    }
}

fn spawn_reader<R: Read + Send + 'static>(
    mut reader: R,
    capture: Capture,
    matcher: Option<String>,
    ready: mpsc::Sender<()>,
    output: Option<PersistentOutputStream>,
    observer: Option<crate::runner::OutputBytesCallback>,
    max_capture_bytes: Option<usize>,
) -> thread::JoinHandle<AppResult<()>> {
    thread::spawn(move || {
        let mut buffer = [0_u8; 4096];
        let mut ready_sent = false;
        let mut match_buffer = Vec::new();
        loop {
            let read = reader.read(&mut buffer).map_err(AppError::internal)?;
            if read == 0 {
                break;
            }
            capture.lock().push(&buffer[..read], max_capture_bytes);
            if let Some(observer) = &observer {
                observer(&buffer[..read]);
            }
            if let Some(output) = output {
                forward_output(output, &buffer[..read])?;
            }
            if !ready_sent && let Some(matcher) = &matcher {
                ready_sent = update_match_buffer(&mut match_buffer, &buffer[..read], matcher);
                if ready_sent {
                    let _ = ready.send(());
                }
            }
        }
        Ok(())
    })
}

fn forward_output(stream: PersistentOutputStream, bytes: &[u8]) -> AppResult<()> {
    match stream {
        PersistentOutputStream::Stdout => {
            let mut stdout = std::io::stdout().lock();
            stdout.write_all(bytes)?;
            stdout.flush()
        }
        PersistentOutputStream::Stderr => {
            let mut stderr = std::io::stderr().lock();
            stderr.write_all(bytes)?;
            stderr.flush()
        }
    }
    .map_err(AppError::internal)
}

fn update_match_buffer(match_buffer: &mut Vec<u8>, bytes: &[u8], matcher: &str) -> bool {
    let needle = matcher.as_bytes();
    if needle.is_empty() {
        return true;
    }
    match_buffer.extend_from_slice(bytes);
    let ready_found = match_buffer
        .windows(needle.len())
        .any(|window| window == needle);
    let keep = needle.len().saturating_sub(1);
    if match_buffer.len() > keep {
        match_buffer.drain(..match_buffer.len() - keep);
    }
    ready_found
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::worker::join_within;
    use std::io::Cursor;
    use std::time::Duration;

    #[test]
    fn capture_and_matcher_helpers_cover_truncation_and_cross_chunk_matches() {
        let capture = new_capture();
        capture.lock().push(b"hello", Some(7));
        capture.lock().push(b" world", Some(7));
        let output = take_capture(&capture);
        assert_eq!(output.bytes, b"hello w");
        assert!(output.truncated);

        let mut buffer = Vec::new();
        assert!(update_match_buffer(&mut buffer, b"", ""));
        assert!(!update_match_buffer(&mut buffer, b"rea", "ready"));
        assert!(update_match_buffer(&mut buffer, b"dy", "ready"));
    }

    #[test]
    fn reader_observes_bytes_without_changing_capture_bounds() {
        let capture = new_capture();
        let observed = Arc::new(parking_lot::Mutex::new(Vec::new()));
        let observer = {
            let observed = Arc::clone(&observed);
            Arc::new(move |chunk: &[u8]| observed.lock().extend_from_slice(chunk))
        };
        let (ready_tx, _ready_rx) = mpsc::channel();

        let reader = spawn_reader(
            Cursor::new(b"hello world".to_vec()),
            Arc::clone(&capture),
            None,
            ready_tx,
            None,
            Some(observer),
            Some(5),
        );
        join_within(Some(reader), Duration::from_secs(1)).expect("reader completes");

        let output = take_capture(&capture);
        assert_eq!(output.bytes, b"hello");
        assert!(output.truncated);
        assert_eq!(&*observed.lock(), b"hello world");
    }
}
