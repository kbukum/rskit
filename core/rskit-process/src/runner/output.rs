use std::time::Duration;

use tokio::{
    io::{AsyncRead, AsyncReadExt},
    task::JoinHandle,
};

use crate::capture::{BoundedOutput, SharedOutput, take_shared};
use crate::{AppError, AppResult, command::DEFAULT_MAX_OUTPUT_BYTES};

use super::observer::{OutputBytesCallback, OutputLineCallback};

pub(in crate::runner) fn spawn_reader<R>(
    reader: Option<R>,
    capture: SharedOutput,
    max_output_bytes: Option<usize>,
    line_callback: Option<OutputLineCallback>,
    bytes_callback: Option<OutputBytesCallback>,
    retain_output: bool,
) -> Option<JoinHandle<AppResult<()>>>
where
    R: AsyncRead + Unpin + Send + 'static,
{
    reader.map(|reader| match (line_callback, bytes_callback) {
        (None, None) => tokio::spawn(read_output(
            reader,
            capture,
            max_output_bytes,
            retain_output,
        )),
        (line_callback, bytes_callback) => tokio::spawn(read_observed_output(
            reader,
            capture,
            max_output_bytes,
            line_callback,
            bytes_callback,
            retain_output,
        )),
    })
}

/// Await a reader/stdin task, bounding the wait by `grace`.
///
/// On the normal path the child has already exited, so the pipe reaches EOF
/// and the task finishes well within `grace`. A task still running after `grace` —
/// a reader blocked because a surviving descendant inherited and holds the pipe open —
/// is aborted (dropping our read end) rather than awaited forever.
/// The bytes it captured before being abandoned remain available through the shared buffer,
/// so the caller still recovers partial output.
pub(in crate::runner) async fn join_within(
    task: Option<JoinHandle<AppResult<()>>>,
    grace: Duration,
) -> AppResult<()> {
    let Some(task) = task else {
        return Ok(());
    };
    let abort = task.abort_handle();
    match tokio::time::timeout(grace, task).await {
        Ok(Ok(result)) => result,
        Ok(Err(join_error)) if join_error.is_cancelled() => Ok(()),
        Ok(Err(join_error)) => Err(AppError::internal(join_error)),
        Err(_elapsed) => {
            abort.abort();
            Ok(())
        }
    }
}

/// Snapshot the captured output for a reader after [`join_within`] drained it.
pub(in crate::runner) fn captured(capture: &SharedOutput) -> BoundedOutput {
    take_shared(capture)
}

async fn read_output<R>(
    mut reader: R,
    capture: SharedOutput,
    max_output_bytes: Option<usize>,
    retain_output: bool,
) -> AppResult<()>
where
    R: AsyncRead + Unpin,
{
    let mut buffer = [0_u8; 4096];

    loop {
        let read = reader.read(&mut buffer).await.map_err(AppError::internal)?;
        if read == 0 {
            break;
        }
        if retain_output {
            capture.lock().push(&buffer[..read], max_output_bytes);
        }
    }

    Ok(())
}

async fn read_observed_output<R>(
    reader: R,
    capture: SharedOutput,
    max_output_bytes: Option<usize>,
    line_callback: Option<OutputLineCallback>,
    bytes_callback: Option<OutputBytesCallback>,
    retain_output: bool,
) -> AppResult<()>
where
    R: AsyncRead + Unpin,
{
    let mut reader = tokio::io::BufReader::new(reader);
    let mut line = Vec::new();
    let max_line_bytes = max_output_bytes.unwrap_or(DEFAULT_MAX_OUTPUT_BYTES);
    let mut line_truncated = false;
    let mut skip_lf_after_cr = false;
    let mut buffer = [0_u8; 4096];

    loop {
        let read = reader.read(&mut buffer).await.map_err(AppError::internal)?;
        if read == 0 {
            if !line.is_empty()
                && !line_truncated
                && let Some(callback) = line_callback.as_ref()
            {
                emit_observed_line(&line, callback);
            }
            break;
        }

        if let Some(callback) = &bytes_callback {
            callback(&buffer[..read]);
        }

        if retain_output {
            capture.lock().push(&buffer[..read], max_output_bytes);
        }

        let Some(line_callback) = line_callback.as_ref() else {
            continue;
        };

        for byte in &buffer[..read] {
            if *byte == b'\n' && skip_lf_after_cr {
                skip_lf_after_cr = false;
                continue;
            }
            skip_lf_after_cr = false;

            if *byte == b'\n' || *byte == b'\r' {
                if !line_truncated {
                    line.push(*byte);
                    emit_observed_line(&line, line_callback);
                }
                line.clear();
                line_truncated = false;
                skip_lf_after_cr = *byte == b'\r';
                continue;
            }

            if line_truncated {
                continue;
            }

            if line.len() < max_line_bytes {
                line.push(*byte);
            } else {
                emit_observed_line(&line, line_callback);
                line.clear();
                line_truncated = true;
            }
        }
    }

    Ok(())
}

fn emit_observed_line(line: &[u8], line_callback: &OutputLineCallback) {
    let observed = String::from_utf8_lossy(line);
    let observed = observed.trim_end_matches(['\r', '\n']);
    line_callback(observed);
}

#[cfg(test)]
mod tests {
    use std::sync::{
        Arc,
        atomic::{AtomicUsize, Ordering},
    };

    use tokio::io::AsyncWriteExt;

    use super::*;
    use crate::capture::shared_output;

    async fn read_to_end<R>(
        reader: R,
        max_output_bytes: Option<usize>,
        line_callback: Option<OutputLineCallback>,
        bytes_callback: Option<OutputBytesCallback>,
        retain_output: bool,
    ) -> AppResult<BoundedOutput>
    where
        R: AsyncRead + Unpin + Send + 'static,
    {
        let capture = shared_output();
        let task = spawn_reader(
            Some(reader),
            capture.clone(),
            max_output_bytes,
            line_callback,
            bytes_callback,
            retain_output,
        );
        join_within(task, Duration::from_secs(5)).await?;
        Ok(captured(&capture))
    }

    #[tokio::test]
    async fn join_within_returns_when_no_task_is_present() {
        join_within(None, Duration::from_millis(10)).await.unwrap();
    }

    #[tokio::test]
    async fn spawned_reader_captures_with_bounds_and_without_retention() {
        let retained = read_to_end(std::io::Cursor::new(b"abcdef"), Some(3), None, None, true)
            .await
            .unwrap();
        assert_eq!(retained.bytes, b"abc");
        assert!(retained.truncated);

        let observed_only =
            read_to_end(std::io::Cursor::new(b"abcdef"), Some(3), None, None, false)
                .await
                .unwrap();
        assert!(observed_only.bytes.is_empty());
        assert!(!observed_only.truncated);

        let zero_limit = read_to_end(std::io::Cursor::new(b"abcdef"), Some(0), None, None, true)
            .await
            .unwrap();
        assert!(zero_limit.bytes.is_empty());
        assert!(zero_limit.truncated);

        let unbounded = read_to_end(std::io::Cursor::new(b"abcdef"), None, None, None, true)
            .await
            .unwrap();
        assert_eq!(unbounded.bytes, b"abcdef");
        assert!(!unbounded.truncated);
    }

    #[tokio::test]
    async fn observed_reader_emits_lines_and_bytes_with_crlf_handling() {
        let lines = Arc::new(parking_lot::Mutex::new(Vec::new()));
        let byte_count = Arc::new(AtomicUsize::new(0));
        let line_callback: OutputLineCallback = {
            let lines = Arc::clone(&lines);
            Arc::new(move |line| lines.lock().push(line.to_string()))
        };
        let bytes_callback: OutputBytesCallback = {
            let byte_count = Arc::clone(&byte_count);
            Arc::new(move |chunk| {
                byte_count.fetch_add(chunk.len(), Ordering::SeqCst);
            })
        };

        let captured = read_to_end(
            std::io::Cursor::new(b"one\r\ntwo\nthree"),
            Some(64),
            Some(line_callback),
            Some(bytes_callback),
            true,
        )
        .await
        .unwrap();

        assert_eq!(captured.bytes, b"one\r\ntwo\nthree");
        assert_eq!(lines.lock().as_slice(), ["one", "two", "three"]);
        assert_eq!(byte_count.load(Ordering::SeqCst), 14);
    }

    #[tokio::test]
    async fn observed_reader_suppresses_overlong_line_tail() {
        let lines = Arc::new(parking_lot::Mutex::new(Vec::new()));
        let callback: OutputLineCallback = {
            let lines = Arc::clone(&lines);
            Arc::new(move |line| lines.lock().push(line.to_string()))
        };

        let captured = read_to_end(
            std::io::Cursor::new(b"abcdef\nok"),
            Some(3),
            Some(callback),
            None,
            true,
        )
        .await
        .unwrap();

        assert_eq!(captured.bytes, b"abc");
        assert!(captured.truncated);
        assert_eq!(lines.lock().as_slice(), ["abc", "ok"]);
    }

    #[tokio::test]
    async fn observed_reader_without_line_callback_only_retains_output() {
        let captured = read_to_end(
            std::io::Cursor::new(b"abcdef"),
            Some(0),
            None,
            Some(Arc::new(|_| {})),
            true,
        )
        .await
        .unwrap();

        assert!(captured.bytes.is_empty());
        assert!(captured.truncated);
    }

    #[tokio::test]
    async fn reader_errors_surface_through_join() {
        let (mut writer, reader) = tokio::io::duplex(8);
        writer.shutdown().await.unwrap();
        drop(writer);

        let capture = shared_output();
        let task = spawn_reader(Some(reader), capture, Some(8), None, None, true);
        assert!(join_within(task, Duration::from_secs(5)).await.is_ok());
    }

    #[tokio::test]
    async fn join_within_surfaces_panicked_tasks() {
        let task = tokio::spawn(async {
            panic!("reader panic");
        });

        let error = join_within(Some(task), Duration::from_secs(5))
            .await
            .unwrap_err();

        assert_eq!(error.code(), crate::ErrorCode::Internal);
    }
}
