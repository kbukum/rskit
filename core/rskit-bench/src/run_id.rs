//! Benchmark run identifier helpers.

use rskit_util::time::format_compact_utc;

pub(crate) fn generate_run_id(name: &str, epoch_seconds: u64) -> String {
    let timestamp = i64::try_from(epoch_seconds)
        .ok()
        .and_then(format_compact_utc)
        .unwrap_or_else(|| "unknown-time".to_string());
    format!("{}-{timestamp}", sanitize_run_name(name))
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
    use super::generate_run_id;

    #[test]
    fn run_id_uses_injected_epoch_seconds() {
        assert_eq!(generate_run_id("eval", 0), "eval-19700101-000000");
        assert_eq!(generate_run_id("eval", 86_400), "eval-19700102-000000");
    }

    #[test]
    fn run_id_sanitizes_name_for_filename_use() {
        assert_eq!(generate_run_id("../a/b", 0), "a-b-19700101-000000");
        assert_eq!(
            generate_run_id(" spaces and/slashes ", 0),
            "spaces-and-slashes-19700101-000000"
        );
        assert_eq!(generate_run_id("...", 0), "run-19700101-000000");
    }
}
