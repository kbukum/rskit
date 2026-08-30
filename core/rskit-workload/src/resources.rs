//! CPU and memory quantity parsing and formatting.
//!
//! Memory parsing is delegated to the canonical [`rskit_util::bytes`] parser (binary suffixes `k`/`ki` … `t`/`ti`);
//! CPU is expressed in cores or millicores and normalized to nanocores.
//! Formatting produces the compact single-letter representation the workload vocabulary uses (`512m`, `2g`).

use rskit_errors::{AppError, AppResult};

const KIB: i64 = 1024;
const MIB: i64 = 1024 * 1024;
const GIB: i64 = 1024 * 1024 * 1024;
const TIB: i64 = 1024 * 1024 * 1024 * 1024;
const PIB: i64 = 1024 * 1024 * 1024 * 1024 * 1024;

const NANOS_PER_CORE: f64 = 1e9;
const NANOS_PER_MILLICORE: f64 = 1e6;

/// Parse a human-readable memory string into bytes.
///
/// Supported suffixes (case-insensitive): `k`/`ki` (KiB), `m`/`mi` (MiB), `g`/`gi` (GiB),
/// `t`/`ti` (TiB), `p`/`pi` (PiB). A bare number is treated as bytes.
/// All units are binary (1024-based).
///
/// # Errors
///
/// Returns [`rskit_errors::ErrorCode::InvalidFormat`] when the string is empty, not a valid quantity,
/// negative, or larger than [`i64::MAX`] bytes.
pub fn parse_memory(s: &str) -> AppResult<i64> {
    let bytes = rskit_util::bytes::parse_bytes(s).ok_or_else(|| {
        AppError::invalid_format("memory", "quantity with optional binary suffix")
    })?;
    i64::try_from(bytes)
        .map_err(|_| AppError::invalid_format("memory", "quantity within i64 range"))
}

/// Parse a human-readable CPU string into nanocores.
///
/// Supported formats (case-insensitive): cores (`"0.5"`, `"1"`) and millicores (`"500m"`).
///
/// # Errors
///
/// Returns [`rskit_errors::ErrorCode::InvalidFormat`] when the string is empty, not a valid number,
/// negative, or at
/// or above the [`i64::MAX`] nanocore boundary (values that would saturate a float-to-int cast).
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
    let nanocores = parsed * scale;
    // `as i64` saturates silently, so reject out-of-range values before casting.
    #[allow(clippy::cast_precision_loss)]
    if nanocores >= i64::MAX as f64 {
        return Err(AppError::invalid_format(
            "cpu",
            "quantity within i64 nanocore range",
        ));
    }
    #[allow(clippy::cast_possible_truncation)]
    // validated finite and in range; fractional nanocores are dropped by design
    Ok(nanocores as i64)
}

/// Format a byte count as a human-readable memory string using binary suffixes.
///
/// Picks the largest binary unit that divides the value exactly
/// so a non-negative result round-trips through [`parse_memory`];
/// a value that is not an exact multiple of any unit (e.g. `1536`) is rendered as raw bytes rather than truncated.
/// Negative inputs (which [`parse_memory`] rejects) render as raw signed integers.
#[must_use]
pub fn format_memory(bytes: i64) -> String {
    for (unit, suffix) in [(PIB, 'p'), (TIB, 't'), (GIB, 'g'), (MIB, 'm'), (KIB, 'k')] {
        if bytes >= unit && bytes % unit == 0 {
            return format!("{}{suffix}", bytes / unit);
        }
    }
    bytes.to_string()
}

/// Format a nanocore count as a human-readable CPU string.
///
/// Whole cores and exact millicores use the compact `N`/`Nm` forms;
/// any other value renders its exact nanocore remainder as fractional cores (up to 9 decimals, trailing zeros trimmed),
/// preserving sign,
/// so a non-negative result round-trips through [`parse_cpu`] instead of collapsing to a truncated `0.000`.
/// [`parse_cpu`] rejects negative inputs.
#[must_use]
pub fn format_cpu(nanocores: i64) -> String {
    if nanocores % 1_000_000_000 == 0 {
        return (nanocores / 1_000_000_000).to_string();
    }
    if nanocores % 1_000_000 == 0 {
        return format!("{}m", nanocores / 1_000_000);
    }
    let sign = if nanocores.is_negative() { "-" } else { "" };
    let magnitude = nanocores.unsigned_abs();
    let whole = magnitude / 1_000_000_000;
    let frac = magnitude % 1_000_000_000;
    let decimals = format!("{frac:09}");
    format!("{sign}{whole}.{}", decimals.trim_end_matches('0'))
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
    fn parse_cpu_rejects_out_of_range() {
        assert_eq!(
            parse_cpu("1e30").unwrap_err().code(),
            ErrorCode::InvalidFormat
        );
    }

    #[test]
    fn format_memory_uses_largest_binary_unit() {
        assert_eq!(format_memory(512), "512");
        assert_eq!(format_memory(2048), "2k");
        assert_eq!(format_memory(3 * MIB), "3m");
        assert_eq!(format_memory(4 * GIB), "4g");
        assert_eq!(format_memory(5 * TIB), "5t");
        assert_eq!(format_memory(6 * PIB), "6p");
        assert_eq!(format_memory(TIB), "1t");
        // Non-multiples fall back to raw bytes rather than truncating.
        assert_eq!(format_memory(1536), "1536");
        assert_eq!(format_memory(GIB + MIB), "1025m");
    }

    #[test]
    fn memory_round_trips_through_format_and_parse() {
        for bytes in [
            512,
            1536,
            2048,
            GIB + MIB,
            3 * MIB,
            4 * GIB,
            5 * TIB,
            6 * PIB,
        ] {
            let formatted = format_memory(bytes);
            assert_eq!(parse_memory(&formatted).unwrap(), bytes, "bytes {bytes}");
        }
    }

    #[test]
    fn format_cpu_prefers_whole_cores_then_millicores() {
        assert_eq!(format_cpu(1_000_000_000), "1");
        assert_eq!(format_cpu(500_000_000), "500m");
        assert_eq!(format_cpu(2_000_000), "2m");
    }

    #[test]
    fn format_cpu_falls_back_to_fractional_cores() {
        assert_eq!(format_cpu(1_500_000), "0.0015");
        assert_eq!(format_cpu(500), "0.0000005");
        assert_eq!(format_cpu(1_000_500_000), "1.0005");
        // Sub-core negative values keep their sign.
        assert_eq!(format_cpu(-500), "-0.0000005");
    }

    #[test]
    fn format_cpu_preserves_sub_millicore_precision() {
        // A value below the millicore grid must not collapse to "0" on round-trip.
        for nanos in [500_i64, 1_500_000, 1_250_000_000, 750, 333_000_000] {
            let formatted = format_cpu(nanos);
            assert_eq!(parse_cpu(&formatted).unwrap(), nanos, "nanos {nanos}");
        }
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
