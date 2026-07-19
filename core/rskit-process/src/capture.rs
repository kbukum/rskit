//! Shared bounded output capture used by every runner (async, blocking, and persistent).
//!
//! All three execution models retain child output up to a configured byte bound,
//! tracking whether the bound was exceeded,
//! and all three append an optional synthetic termination diagnostic to the retained stderr.
//! This module owns that logic once so the runners cannot drift apart.

use std::sync::Arc;

use parking_lot::Mutex;

/// Retained output bytes plus whether the configured bound was exceeded.
#[derive(Debug, Default, Clone)]
pub(crate) struct BoundedOutput {
    /// Retained bytes, capped at the configured bound.
    pub(crate) bytes: Vec<u8>,
    /// Whether output beyond the bound was dropped.
    pub(crate) truncated: bool,
}

impl BoundedOutput {
    /// Append `chunk`, retaining at most `max_bytes` total; `None` is unbounded.
    ///
    /// Sets [`truncated`](Self::truncated) when any input byte is dropped.
    pub(crate) fn push(&mut self, chunk: &[u8], max_bytes: Option<usize>) {
        let Some(limit) = max_bytes else {
            self.bytes.extend_from_slice(chunk);
            return;
        };
        let remaining = limit.saturating_sub(self.bytes.len());
        let kept = remaining.min(chunk.len());
        self.bytes.extend_from_slice(&chunk[..kept]);
        if kept < chunk.len() {
            self.truncated = true;
        }
    }
}

/// A [`BoundedOutput`] shared between a reader task/thread and the orchestrator.
///
/// The reader owns the write side and the orchestrator snapshots it after the child exits,
/// so a bounded drain can recover whatever was captured even when the reader is abandoned (aborted task or detached thread) because a surviving descendant still holds the pipe open.
pub(crate) type SharedOutput = Arc<Mutex<BoundedOutput>>;

/// Create an empty shared output buffer.
pub(crate) fn shared_output() -> SharedOutput {
    Arc::new(Mutex::new(BoundedOutput::default()))
}

/// Snapshot the current contents of a shared output buffer.
pub(crate) fn take_shared(shared: &SharedOutput) -> BoundedOutput {
    shared.lock().clone()
}

/// Append `extra` to `dst` on its own line, bounded by `max_bytes`.
///
/// Returns whether any byte of `extra` was dropped to stay within the bound.
/// Used to fold a synthetic termination diagnostic into retained stderr.
pub(crate) fn append_line_bounded(
    dst: &mut Vec<u8>,
    extra: &[u8],
    max_bytes: Option<usize>,
) -> bool {
    let Some(limit) = max_bytes else {
        if !dst.is_empty() {
            dst.push(b'\n');
        }
        dst.extend_from_slice(extra);
        return false;
    };

    if dst.len() >= limit {
        return true;
    }

    if !dst.is_empty() && dst.len() + 1 < limit {
        dst.push(b'\n');
    }

    let remaining = limit.saturating_sub(dst.len());
    if extra.len() > remaining {
        dst.extend_from_slice(&extra[..remaining]);
        true
    } else {
        dst.extend_from_slice(extra);
        false
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn push_retains_up_to_the_bound_and_flags_truncation() {
        let mut output = BoundedOutput::default();
        output.push(b"hello", Some(7));
        output.push(b" world", Some(7));
        assert_eq!(output.bytes, b"hello w");
        assert!(output.truncated);

        let mut unbounded = BoundedOutput::default();
        unbounded.push(b"abcdef", None);
        assert_eq!(unbounded.bytes, b"abcdef");
        assert!(!unbounded.truncated);

        let mut zero = BoundedOutput::default();
        zero.push(b"abc", Some(0));
        assert!(zero.bytes.is_empty());
        assert!(zero.truncated);
    }

    #[test]
    fn append_line_bounded_preserves_separator_and_reports_truncation() {
        let mut unbounded = b"err".to_vec();
        assert!(!append_line_bounded(&mut unbounded, b"tail", None));
        assert_eq!(unbounded, b"err\ntail");

        let mut full = b"abc".to_vec();
        assert!(append_line_bounded(&mut full, b"tail", Some(3)));
        assert_eq!(full, b"abc");

        let mut partial = b"a".to_vec();
        assert!(append_line_bounded(&mut partial, b"bcdef", Some(4)));
        assert_eq!(partial, b"a\nbc");

        let mut fits = b"a".to_vec();
        assert!(!append_line_bounded(&mut fits, b"b", Some(4)));
        assert_eq!(fits, b"a\nb");
    }

    #[test]
    fn take_shared_snapshots_without_consuming() {
        let shared = shared_output();
        shared.lock().push(b"data", Some(16));
        let snapshot = take_shared(&shared);
        assert_eq!(snapshot.bytes, b"data");
        assert!(!snapshot.truncated);
        assert_eq!(shared.lock().bytes, b"data");
    }
}
