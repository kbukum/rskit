//! Benchmark run identifier helpers.

use rskit_util::time::format_compact_utc;

/// Builds a run identifier of the form `<tag|run>_<ts>_<uuid8>`: the run tag (or `run` when
/// untagged) followed by a compact UTC timestamp and a short unique suffix, so two runs that
/// share a tag and second still map to distinct storage files. The suffix is injected so runs
/// are reproducible under a fixed clock and suffix source.
pub(crate) fn generate_run_id(name: &str, epoch_seconds: u64, suffix: &str) -> String {
    let timestamp = i64::try_from(epoch_seconds)
        .ok()
        .and_then(format_compact_utc)
        .unwrap_or_else(|| "unknown-time".to_string());
    format!("{}_{timestamp}_{suffix}", sanitize_run_name(name))
}

/// Production default run-id suffix source: the first 8 characters of a random UUID v4.
pub(crate) fn random_id_suffix() -> String {
    uuid::Uuid::new_v4().simple().to_string()[..8].to_string()
}

fn sanitize_run_name(name: &str) -> String {
    let mut sanitized = String::new();
    let mut last_was_separator = false;

    for ch in name.chars() {
        if ch.is_ascii_alphanumeric() {
            sanitized.push(ch);
            last_was_separator = false;
        } else if (ch == '-' || ch == '_') && !sanitized.is_empty() && !last_was_separator {
            sanitized.push(ch);
            last_was_separator = true;
        } else if !sanitized.is_empty() && !last_was_separator {
            sanitized.push('-');
            last_was_separator = true;
        }
    }

    while sanitized.ends_with('-') || sanitized.ends_with('_') {
        sanitized.pop();
    }

    if sanitized.is_empty() {
        "run".to_string()
    } else {
        sanitized
    }
}

#[cfg(test)]
mod tests {
    use super::{generate_run_id, random_id_suffix};

    #[test]
    fn run_id_uses_injected_epoch_seconds_and_suffix() {
        assert_eq!(
            generate_run_id("eval", 0, "abcd1234"),
            "eval_19700101-000000_abcd1234"
        );
        assert_eq!(
            generate_run_id("eval", 86_400, "abcd1234"),
            "eval_19700102-000000_abcd1234"
        );
    }

    #[test]
    fn run_id_sanitizes_name_for_filename_use() {
        assert_eq!(generate_run_id("../a/b", 0, "s"), "a-b_19700101-000000_s");
        assert_eq!(
            generate_run_id(" spaces and/slashes ", 0, "s"),
            "spaces-and-slashes_19700101-000000_s"
        );
        assert_eq!(generate_run_id("...", 0, "s"), "run_19700101-000000_s");
    }

    #[test]
    fn random_id_suffix_is_eight_hex_chars() {
        let suffix = random_id_suffix();
        assert_eq!(suffix.len(), 8);
        assert!(suffix.chars().all(|c| c.is_ascii_hexdigit()));
    }
}
