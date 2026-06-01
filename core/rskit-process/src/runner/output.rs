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
