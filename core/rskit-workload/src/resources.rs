//! CPU and memory quantity parsing and formatting.
//!
//! Mirrors gokit's `workload` resource helpers. Memory parsing is delegated to
//! the canonical [`rskit_util::bytes`] parser (binary suffixes `k`/`ki` …
//! `t`/`ti`); CPU is expressed in cores or millicores and normalized to
//! nanocores. Formatting produces the compact single-letter representation the
//! workload vocabulary uses (`512m`, `2g`).

use rskit_errors::{AppError, AppResult};

const KIB: i64 = 1024;
const MIB: i64 = 1024 * 1024;
const GIB: i64 = 1024 * 1024 * 1024;

const NANOS_PER_CORE: f64 = 1e9;
const NANOS_PER_MILLICORE: f64 = 1e6;

/// Parse a human-readable memory string into bytes.
///
/// Supported suffixes (case-insensitive): `k`/`ki` (KiB), `m`/`mi` (MiB),
/// `g`/`gi` (GiB), `t`/`ti` (TiB), `p`/`pi` (PiB). A bare number is treated as
/// bytes. All units are binary (1024-based).
///
/// # Errors
///
/// Returns [`rskit_errors::ErrorCode::InvalidFormat`] when the string is empty,
/// not a valid quantity, negative, or larger than [`i64::MAX`] bytes.
pub fn parse_memory(s: &str) -> AppResult<i64> {
    let bytes = rskit_util::bytes::parse_bytes(s).ok_or_else(|| {
        AppError::invalid_format("memory", "quantity with optional binary suffix")
    })?;
    i64::try_from(bytes)
        .map_err(|_| AppError::invalid_format("memory", "quantity within i64 range"))
}

/// Parse a human-readable CPU string into nanocores.
///
/// Supported formats (case-insensitive): cores (`"0.5"`, `"1"`) and millicores
/// (`"500m"`).
///
/// # Errors
///
/// Returns [`rskit_errors::ErrorCode::InvalidFormat`] when the string is empty,
/// not a valid number, or negative.
pub fn parse_cpu(s: &str) -> AppResult<i64> {
    let lower = s.trim().to_lowercase();
    if lower.is_empty() {
        return Err(AppError::invalid_format("cpu", "non-empty quantity"));
    }

    let (value, scale) = lower.strip_suffix('m').map_or_else(
        || (lower.as_str(), NANOS_PER_CORE),
        |millis| (millis, NANOS_PER_MILLICORE),
    );
    let parsed: f64 = value
        .parse()
        .map_err(|_| AppError::invalid_format("cpu", "number in cores or millicores"))?;
    if parsed < 0.0 || !parsed.is_finite() {
        return Err(AppError::invalid_format(
            "cpu",
            "non-negative finite quantity",
        ));
    }
    #[allow(clippy::cast_possible_truncation)]
    // validated finite, fractional nanocores are dropped by design
    Ok((parsed * scale) as i64)
}

/// Format a byte count as a human-readable memory string using binary suffixes.
#[must_use]
pub fn format_memory(bytes: i64) -> String {
    match bytes {
        b if b >= GIB => format!("{}g", b / GIB),
        b if b >= MIB => format!("{}m", b / MIB),
        b if b >= KIB => format!("{}k", b / KIB),
        b => b.to_string(),
    }
}

/// Format a nanocore count as a human-readable CPU string.
#[must_use]
pub fn format_cpu(nanocores: i64) -> String {
    if nanocores % 1_000_000_000 == 0 {
        return (nanocores / 1_000_000_000).to_string();
    }
    if nanocores % 1_000_000 == 0 {
        return format!("{}m", nanocores / 1_000_000);
    }
    #[allow(clippy::cast_precision_loss)] // human-readable formatting; precision loss is acceptable
    let fractional = nanocores as f64 / NANOS_PER_CORE;
    format!("{fractional:.3}")
}

#[cfg(test)]
mod tests {
    use super::*;
    use rskit_errors::ErrorCode;

    #[test]
    fn parse_memory_handles_binary_suffixes() {
        assert_eq!(parse_memory("512").unwrap(), 512);
        assert_eq!(parse_memory("1k").unwrap(), 1024);
        assert_eq!(parse_memory("2Mi").unwrap(), 2 * MIB);
        assert_eq!(parse_memory("1g").unwrap(), GIB);
        assert_eq!(parse_memory(" 1Ti ").unwrap(), 1024_i64.pow(4));
    }

    #[test]
    fn parse_memory_rejects_bad_input() {
        assert_eq!(
            parse_memory("").unwrap_err().code(),
            ErrorCode::InvalidFormat
        );
        assert_eq!(
            parse_memory("abc").unwrap_err().code(),
            ErrorCode::InvalidFormat
        );
        assert_eq!(
            parse_memory("-5").unwrap_err().code(),
            ErrorCode::InvalidFormat
        );
    }

    #[test]
    fn parse_memory_rejects_overflow() {
        assert_eq!(
            parse_memory("9223372036854775807t").unwrap_err().code(),
            ErrorCode::InvalidFormat
        );
    }

    #[test]
    fn parse_cpu_handles_cores_and_millicores() {
        assert_eq!(parse_cpu("1").unwrap(), 1_000_000_000);
        assert_eq!(parse_cpu("0.5").unwrap(), 500_000_000);
        assert_eq!(parse_cpu("500m").unwrap(), 500_000_000);
    }

    #[test]
    fn parse_cpu_rejects_bad_input() {
        assert_eq!(parse_cpu("").unwrap_err().code(), ErrorCode::InvalidFormat);
        assert_eq!(
            parse_cpu("fast").unwrap_err().code(),
            ErrorCode::InvalidFormat
        );
        assert_eq!(
            parse_cpu("-1").unwrap_err().code(),
            ErrorCode::InvalidFormat
        );
    }

    #[test]
    fn format_memory_uses_largest_binary_unit() {
        assert_eq!(format_memory(512), "512");
        assert_eq!(format_memory(2048), "2k");
        assert_eq!(format_memory(3 * MIB), "3m");
        assert_eq!(format_memory(4 * GIB), "4g");
    }

    #[test]
    fn format_cpu_prefers_whole_cores_then_millicores() {
        assert_eq!(format_cpu(1_000_000_000), "1");
        assert_eq!(format_cpu(500_000_000), "500m");
        assert_eq!(format_cpu(2_000_000), "2m");
    }

    #[test]
    fn format_cpu_falls_back_to_fractional_cores() {
        assert_eq!(format_cpu(1_500_000), "0.002");
    }

    #[test]
    fn cpu_round_trips_through_format_and_parse() {
        for input in ["1", "0.5", "500m", "2"] {
            let nanos = parse_cpu(input).unwrap();
            let formatted = format_cpu(nanos);
            assert_eq!(parse_cpu(&formatted).unwrap(), nanos, "input {input}");
        }
    }
}
