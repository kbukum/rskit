use std::io;

use tokio::{
    io::{AsyncRead, AsyncReadExt},
    task::JoinHandle,
};

use crate::{AppError, AppResult, command::DEFAULT_MAX_OUTPUT_BYTES};

use super::observer::{OutputBytesCallback, OutputLineCallback};

#[derive(Debug)]
pub(in crate::runner) struct CapturedOutput {
    pub(in crate::runner) bytes: Vec<u8>,
    pub(in crate::runner) truncated: bool,
}

pub(in crate::runner) fn spawn_reader<R>(
    reader: Option<R>,
    max_output_bytes: Option<usize>,
    line_callback: Option<OutputLineCallback>,
    bytes_callback: Option<OutputBytesCallback>,
    retain_output: bool,
) -> Option<JoinHandle<io::Result<CapturedOutput>>>
where
    R: AsyncRead + Unpin + Send + 'static,
{
    reader.map(|reader| match (line_callback, bytes_callback) {
        (None, None) => tokio::spawn(read_output(reader, max_output_bytes, retain_output)),
        (line_callback, bytes_callback) => tokio::spawn(read_observed_output(
            reader,
            max_output_bytes,
            line_callback,
            bytes_callback,
            retain_output,
        )),
    })
}

pub(in crate::runner) async fn collect_reader(
    task: Option<JoinHandle<io::Result<CapturedOutput>>>,
) -> AppResult<CapturedOutput> {
    match task {
        Some(task) => task
            .await
            .map_err(AppError::internal)?
            .map_err(AppError::internal),
        None => Ok(CapturedOutput {
            bytes: Vec::new(),
            truncated: false,
        }),
    }
}

async fn read_output<R>(
    mut reader: R,
    max_output_bytes: Option<usize>,
    retain_output: bool,
) -> io::Result<CapturedOutput>
where
    R: AsyncRead + Unpin,
{
    let mut captured = Vec::new();
    let mut buffer = [0_u8; 4096];
    let mut remaining = max_output_bytes.unwrap_or(usize::MAX);
    let mut truncated = false;

    loop {
        let read = reader.read(&mut buffer).await?;
        if read == 0 {
            break;
        }
        if retain_output && remaining > 0 {
            let to_copy = remaining.min(read);
            captured.extend_from_slice(&buffer[..to_copy]);
            remaining -= to_copy;
            if to_copy < read {
                truncated = true;
            }
        } else if retain_output {
            truncated = true;
        }
    }

    Ok(CapturedOutput {
        bytes: captured,
        truncated,
    })
}

async fn read_observed_output<R>(
    reader: R,
    max_output_bytes: Option<usize>,
    line_callback: Option<OutputLineCallback>,
    bytes_callback: Option<OutputBytesCallback>,
    retain_output: bool,
) -> io::Result<CapturedOutput>
where
    R: AsyncRead + Unpin,
{
    let mut reader = tokio::io::BufReader::new(reader);
    let mut captured = Vec::new();
    let mut remaining = max_output_bytes.unwrap_or(usize::MAX);
    let mut line = Vec::new();
    let max_line_bytes = max_output_bytes.unwrap_or(DEFAULT_MAX_OUTPUT_BYTES);
    let mut line_truncated = false;
    let mut skip_lf_after_cr = false;
    let mut buffer = [0_u8; 4096];
    let mut capture_truncated = false;

    loop {
        let read = reader.read(&mut buffer).await?;
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

        if retain_output && remaining > 0 {
            let to_copy = remaining.min(read);
            captured.extend_from_slice(&buffer[..to_copy]);
            remaining -= to_copy;
            if to_copy < read {
                capture_truncated = true;
            }
        } else if retain_output {
            capture_truncated = true;
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

    Ok(CapturedOutput {
        bytes: captured,
        truncated: capture_truncated,
    })
}

fn emit_observed_line(line: &[u8], line_callback: &OutputLineCallback) {
    let observed = String::from_utf8_lossy(line);
    let observed = observed.trim_end_matches(['\r', '\n']);
    line_callback(observed);
}

pub(in crate::runner) fn append_bounded_stderr(
    stderr: &mut Vec<u8>,
    extra: &[u8],
    max_output_bytes: Option<usize>,
) -> bool {
    let Some(limit) = max_output_bytes else {
        if !stderr.is_empty() {
            stderr.push(b'\n');
        }
        stderr.extend_from_slice(extra);
        return false;
    };

    if stderr.len() >= limit {
        return true;
    }

    let mut truncated = false;
    if !stderr.is_empty() && stderr.len() + 1 < limit {
        stderr.push(b'\n');
    }

    let remaining = limit.saturating_sub(stderr.len());
    if extra.len() > remaining {
        stderr.extend_from_slice(&extra[..remaining]);
        truncated = true;
    } else {
        stderr.extend_from_slice(extra);
    }
    truncated
}

#[cfg(test)]
mod tests {
    use std::sync::{
        Arc,
        atomic::{AtomicUsize, Ordering},
    };

    use tokio::io::AsyncWriteExt;

    use super::*;

    #[tokio::test]
    async fn collect_reader_returns_empty_capture_when_no_task_is_present() {
        let captured = collect_reader(None).await.unwrap();
        assert!(captured.bytes.is_empty());
        assert!(!captured.truncated);
    }

    #[tokio::test]
    async fn spawned_reader_captures_with_bounds_and_without_retention() {
        let retained = spawn_reader(
            Some(tokio::io::BufReader::new(std::io::Cursor::new(b"abcdef"))),
            Some(3),
            None,
            None,
            true,
        );
        let retained = collect_reader(retained).await.unwrap();
        assert_eq!(retained.bytes, b"abc");
        assert!(retained.truncated);

        let observed_only = spawn_reader(
            Some(tokio::io::BufReader::new(std::io::Cursor::new(b"abcdef"))),
            Some(3),
            None,
            None,
            false,
        );
        let observed_only = collect_reader(observed_only).await.unwrap();
        assert!(observed_only.bytes.is_empty());
        assert!(!observed_only.truncated);

        let zero_limit = spawn_reader(
            Some(tokio::io::BufReader::new(std::io::Cursor::new(b"abcdef"))),
            Some(0),
            None,
            None,
            true,
        );
        let zero_limit = collect_reader(zero_limit).await.unwrap();
        assert!(zero_limit.bytes.is_empty());
        assert!(zero_limit.truncated);

        let unbounded = spawn_reader(
            Some(tokio::io::BufReader::new(std::io::Cursor::new(b"abcdef"))),
            None,
            None,
            None,
            true,
        );
        let unbounded = collect_reader(unbounded).await.unwrap();
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

        let task = spawn_reader(
            Some(tokio::io::BufReader::new(std::io::Cursor::new(
                b"one\r\ntwo\nthree",
            ))),
            Some(64),
            Some(line_callback),
            Some(bytes_callback),
            true,
        );
        let captured = collect_reader(task).await.unwrap();

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

        let task = spawn_reader(
            Some(tokio::io::BufReader::new(std::io::Cursor::new(
                b"abcdef\nok",
            ))),
            Some(3),
            Some(callback),
            None,
            true,
        );
        let captured = collect_reader(task).await.unwrap();

        assert_eq!(captured.bytes, b"abc");
        assert!(captured.truncated);
        assert_eq!(lines.lock().as_slice(), ["abc", "ok"]);
    }

    #[tokio::test]
    async fn observed_reader_without_line_callback_only_retains_output() {
        let task = spawn_reader(
            Some(tokio::io::BufReader::new(std::io::Cursor::new(b"abcdef"))),
            Some(0),
            None,
            Some(Arc::new(|_| {})),
            true,
        );
        let captured = collect_reader(task).await.unwrap();

        assert!(captured.bytes.is_empty());
        assert!(captured.truncated);
    }

    #[tokio::test]
    async fn collect_reader_maps_reader_errors() {
        let (mut writer, reader) = tokio::io::duplex(8);
        writer.shutdown().await.unwrap();
        drop(writer);

        let task = spawn_reader(Some(reader), Some(8), None, None, true);
        assert!(collect_reader(task).await.is_ok());
    }

    #[test]
    fn bounded_stderr_appends_separator_and_reports_truncation() {
        let mut stderr = b"err".to_vec();
        assert!(!append_bounded_stderr(&mut stderr, b"tail", None));
        assert_eq!(stderr, b"err\ntail");

        let mut full = b"abc".to_vec();
        assert!(append_bounded_stderr(&mut full, b"tail", Some(3)));
        assert_eq!(full, b"abc");

        let mut partial = b"a".to_vec();
        assert!(append_bounded_stderr(&mut partial, b"bcdef", Some(4)));
        assert_eq!(partial, b"a\nbc");

        let mut fits = b"a".to_vec();
        assert!(!append_bounded_stderr(&mut fits, b"b", Some(4)));
        assert_eq!(fits, b"a\nb");
    }
}
