use std::time::Duration;

use tokio::{
    io::{AsyncRead, AsyncReadExt},
    task::JoinHandle,
};

use crate::capture::{BoundedOutput, SharedOutput, take_shared};
use crate::{AppError, AppResult, command::DEFAULT_MAX_OUTPUT_BYTES};

use super::observer::{OutputBytesSink, OutputLineCallback};

const READER_POLL_INTERVAL: Duration = Duration::from_millis(10);

pub(in crate::runner) fn spawn_reader<R>(
    reader: Option<R>,
    capture: SharedOutput,
    max_output_bytes: Option<usize>,
    line_callback: Option<OutputLineCallback>,
    bytes_sink: Option<OutputBytesSink>,
    retain_output: bool,
) -> Option<JoinHandle<AppResult<()>>>
where
    R: AsyncRead + Unpin + Send + 'static,
{
    reader.map(|reader| match (line_callback, bytes_sink) {
        (None, None) => tokio::spawn(read_output(
            reader,
            capture,
            max_output_bytes,
            retain_output,
        )),
        (line_callback, bytes_sink) => tokio::spawn(read_observed_output(
            reader,
            capture,
            max_output_bytes,
            line_callback,
            bytes_sink,
            retain_output,
        )),
    })
}

/// Wait up to `grace` for every reader to finish, without consuming the handles.
///
/// Returns `false` when a reader is still running at the deadline, which after the leader exited usually means a surviving descendant still holds the pipe open.
pub(in crate::runner) async fn readers_finish_within(
    tasks: &[Option<&JoinHandle<AppResult<()>>>],
    grace: Duration,
) -> bool {
    let deadline = tokio::time::Instant::now() + grace;
    loop {
        if tasks.iter().flatten().all(|task| task.is_finished()) {
            return true;
        }
        let remaining = deadline.saturating_duration_since(tokio::time::Instant::now());
        if remaining.is_zero() {
            return false;
        }
        tokio::time::sleep(remaining.min(READER_POLL_INTERVAL)).await;
    }
}

/// Await a reader/stdin task, bounding the wait by `grace`.
///
/// A task still running after `grace` is aborted and joined rather than detached. When byte delivery is required, expiry is a timeout error: a slow sink or an inherited pipe must not silently lose output. Capture-only callers can recover partial bytes from the shared buffer.
pub(in crate::runner) async fn join_within(
    task: Option<JoinHandle<AppResult<()>>>,
    grace: Duration,
    require_delivery: bool,
) -> AppResult<()> {
    let Some(mut task) = task else {
        return Ok(());
    };
    match tokio::time::timeout(grace, &mut task).await {
        Ok(Ok(result)) => result,
        Ok(Err(join_error)) if join_error.is_cancelled() && !require_delivery => Ok(()),
        Ok(Err(join_error)) => Err(AppError::internal(join_error)),
        Err(elapsed) => {
            task.abort();
            match task.await {
                Ok(result) => result?,
                Err(error) if error.is_cancelled() => {}
                Err(error) => return Err(AppError::internal(error)),
            }
            if require_delivery {
                Err(AppError::new(
                    crate::ErrorCode::Timeout,
                    "process output delivery did not complete within the post-exit grace period",
                )
                .with_cause(elapsed))
            } else {
                Ok(())
            }
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
    bytes_sink: Option<OutputBytesSink>,
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

        if let Some(sink) = &bytes_sink {
            sink(buffer[..read].to_vec()).await?;
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
        bytes_sink: Option<OutputBytesSink>,
        retain_output: bool,
    ) -> AppResult<BoundedOutput>
    where
        R: AsyncRead + Unpin + Send + 'static,
    {
        let require_delivery = bytes_sink.is_some();
        let capture = shared_output();
        let task = spawn_reader(
            Some(reader),
            capture.clone(),
            max_output_bytes,
            line_callback,
            bytes_sink,
            retain_output,
        );
        join_within(task, Duration::from_secs(5), require_delivery).await?;
        Ok(captured(&capture))
    }

    #[tokio::test]
    async fn join_within_returns_when_no_task_is_present() {
        join_within(None, Duration::from_millis(10), false)
            .await
            .unwrap();
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
        let bytes_sink: OutputBytesSink = {
            let byte_count = Arc::clone(&byte_count);
            Arc::new(move |chunk: Vec<u8>| {
                byte_count.fetch_add(chunk.len(), Ordering::SeqCst);
                Box::pin(async { Ok(()) })
            })
        };

        let captured = read_to_end(
            std::io::Cursor::new(b"one\r\ntwo\nthree"),
            Some(64),
            Some(line_callback),
            Some(bytes_sink),
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
            Some(Arc::new(
                |_: Vec<u8>| -> crate::runner::observer::OutputBytesDelivery {
                    Box::pin(async { Ok(()) })
                },
            )),
            true,
        )
        .await
        .unwrap();

        assert!(captured.bytes.is_empty());
        assert!(captured.truncated);
    }

    #[tokio::test(start_paused = true)]
    async fn observed_reader_awaits_a_slow_bytes_sink_instead_of_dropping() {
        let (mut writer, reader) = tokio::io::duplex(16);
        let (sink_tx, mut sink_rx) = tokio::sync::mpsc::channel::<Vec<u8>>(1);
        let sink: OutputBytesSink = Arc::new(move |chunk: Vec<u8>| {
            let sink_tx = sink_tx.clone();
            Box::pin(async move {
                sink_tx.send(chunk).await.unwrap();
                Ok(())
            })
        });
        let task = spawn_reader(Some(reader), shared_output(), None, None, Some(sink), false);
        let payload: Vec<u8> = (0..=255_u8).cycle().take(4096).collect();
        let writing = tokio::spawn({
            let payload = payload.clone();
            async move {
                writer.write_all(&payload).await.unwrap();
                writer.shutdown().await.unwrap();
            }
        });

        // Nothing drains the sink yet, so the reader stops reading and the writer stalls.
        tokio::time::sleep(Duration::from_secs(1)).await;
        assert!(
            !writing.is_finished(),
            "a full sink must backpressure the writer"
        );

        let mut received = Vec::new();
        while let Some(chunk) = sink_rx.recv().await {
            received.extend_from_slice(&chunk);
            if received.len() == payload.len() {
                break;
            }
        }
        writing.await.unwrap();
        join_within(task, Duration::from_secs(5), true)
            .await
            .unwrap();
        assert_eq!(received, payload, "every byte arrives, in order");
    }

    #[tokio::test(start_paused = true)]
    async fn stalled_byte_delivery_is_an_error_instead_of_success() {
        let sink: OutputBytesSink =
            Arc::new(|_| Box::pin(async { std::future::pending::<AppResult<()>>().await }));
        let task = spawn_reader(
            Some(std::io::Cursor::new(b"last diagnostic")),
            shared_output(),
            None,
            None,
            Some(sink),
            false,
        );
        let error = join_within(task, Duration::from_millis(10), true)
            .await
            .expect_err("incomplete delivery must not report success");
        assert_eq!(error.code(), crate::ErrorCode::Timeout);
    }

    #[tokio::test]
    async fn byte_sink_errors_keep_their_code_and_cause() {
        let sink: OutputBytesSink = Arc::new(|_| {
            Box::pin(async {
                Err(
                    AppError::new(crate::ErrorCode::Conflict, "destination failed")
                        .with_cause(std::io::Error::other("disk unavailable")),
                )
            })
        });
        let error = read_to_end(
            std::io::Cursor::new(b"diagnostic"),
            None,
            None,
            Some(sink),
            false,
        )
        .await
        .expect_err("sink failure");
        assert_eq!(error.code(), crate::ErrorCode::Conflict);
        assert!(error.to_string().contains("destination failed"));
        assert!(std::error::Error::source(&error).is_some());
    }

    #[tokio::test]
    async fn reader_errors_surface_through_join() {
        let (mut writer, reader) = tokio::io::duplex(8);
        writer.shutdown().await.unwrap();
        drop(writer);

        let capture = shared_output();
        let task = spawn_reader(Some(reader), capture, Some(8), None, None, true);
        assert!(
            join_within(task, Duration::from_secs(5), false)
                .await
                .is_ok()
        );
    }

    #[tokio::test]
    async fn join_within_surfaces_panicked_tasks() {
        let task = tokio::spawn(async {
            panic!("reader panic");
        });

        let error = join_within(Some(task), Duration::from_secs(5), false)
            .await
            .unwrap_err();

        assert_eq!(error.code(), crate::ErrorCode::Internal);
    }
}
