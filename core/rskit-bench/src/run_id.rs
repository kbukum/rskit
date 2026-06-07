//! Benchmark run identifier helpers.

use rskit_util::time::format_compact_utc;

pub(crate) fn generate_run_id(name: &str, epoch_seconds: u64) -> String {
    let timestamp = i64::try_from(epoch_seconds)
        .ok()
        .and_then(format_compact_utc)
        .unwrap_or_else(|| "unknown-time".to_string());
    format!("{name}-{timestamp}")
}

#[cfg(test)]
mod tests {
    use super::generate_run_id;

    #[test]
    fn run_id_uses_injected_epoch_seconds() {
        assert_eq!(generate_run_id("eval", 0), "eval-19700101-000000");
        assert_eq!(generate_run_id("eval", 86_400), "eval-19700102-000000");
    }
}
