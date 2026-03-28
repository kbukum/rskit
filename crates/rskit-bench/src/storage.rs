//! Persistent storage helpers for bench run results.

use crate::report::RunResult;
use rskit_errors::{AppError, AppResult, ErrorCode};
use std::path::Path;

/// Save a run result to a JSON file, returns the run ID.
pub fn save_run(dir: &Path, result: &RunResult) -> AppResult<String> {
    std::fs::create_dir_all(dir)?;
    let path = dir.join(format!("{}.json", result.run_id));
    let json = serde_json::to_string_pretty(result)?;
    std::fs::write(&path, json)?;
    Ok(result.run_id.clone())
}

/// Load a run result by ID.
pub fn load_run(dir: &Path, run_id: &str) -> AppResult<RunResult> {
    let path = dir.join(format!("{run_id}.json"));
    let content = std::fs::read_to_string(&path)
        .map_err(|e| AppError::new(ErrorCode::NotFound, format!("Run not found: {} ({})", run_id, e)))?;
    Ok(serde_json::from_str(&content)?)
}

/// Load the most recently modified run.
pub fn latest_run(dir: &Path) -> AppResult<RunResult> {
    let mut entries: Vec<_> = std::fs::read_dir(dir)?
        .filter_map(|e| e.ok())
        .filter(|e| e.path().extension().is_some_and(|ext| ext == "json"))
        .collect();

    entries.sort_by(|a, b| {
        let ma = a
            .metadata()
            .and_then(|m| m.modified())
            .unwrap_or(std::time::SystemTime::UNIX_EPOCH);
        let mb = b
            .metadata()
            .and_then(|m| m.modified())
            .unwrap_or(std::time::SystemTime::UNIX_EPOCH);
        mb.cmp(&ma)
    });

    let latest = entries
        .first()
        .ok_or_else(|| AppError::new(ErrorCode::NotFound, format!("No runs found in {}", dir.display())))?;

    let run_id = latest
        .path()
        .file_stem()
        .unwrap_or_default()
        .to_string_lossy()
        .to_string();
    load_run(dir, &run_id)
}

/// List all run IDs in the results directory.
pub fn list_runs(dir: &Path) -> AppResult<Vec<String>> {
    let mut runs = Vec::new();
    if !dir.exists() {
        return Ok(runs);
    }
    for entry in std::fs::read_dir(dir)? {
        let entry = entry?;
        if entry.path().extension().is_some_and(|ext| ext == "json") {
            if let Some(stem) = entry.path().file_stem() {
                runs.push(stem.to_string_lossy().to_string());
            }
        }
    }
    runs.sort();
    Ok(runs)
}

/// Generate a timestamped run ID.
pub fn generate_run_id(name: &str) -> String {
    let now = chrono_like_timestamp();
    format!("{name}-{now}")
}

fn chrono_like_timestamp() -> String {
    use std::time::SystemTime;
    let duration = SystemTime::now()
        .duration_since(SystemTime::UNIX_EPOCH)
        .unwrap_or_default();
    let secs = duration.as_secs();
    // Simple date-time from epoch seconds (good enough for unique IDs)
    let days = secs / 86400;
    let remaining = secs % 86400;
    let hours = remaining / 3600;
    let minutes = (remaining % 3600) / 60;
    let seconds = remaining % 60;
    // Approximate date from days since epoch (1970-01-01)
    let (year, month, day) = days_to_date(days);
    format!("{year:04}{month:02}{day:02}-{hours:02}{minutes:02}{seconds:02}")
}

fn days_to_date(days: u64) -> (u64, u64, u64) {
    // Simplified date calculation from days since epoch
    let mut y = 1970;
    let mut remaining = days;
    loop {
        let days_in_year = if is_leap(y) { 366 } else { 365 };
        if remaining < days_in_year {
            break;
        }
        remaining -= days_in_year;
        y += 1;
    }
    let months: [u64; 12] = if is_leap(y) {
        [31, 29, 31, 30, 31, 30, 31, 31, 30, 31, 30, 31]
    } else {
        [31, 28, 31, 30, 31, 30, 31, 31, 30, 31, 30, 31]
    };
    let mut m = 1;
    for &dim in &months {
        if remaining < dim {
            break;
        }
        remaining -= dim;
        m += 1;
    }
    (y, m, remaining + 1)
}

fn is_leap(y: u64) -> bool {
    y % 4 == 0 && (y % 100 != 0 || y % 400 == 0)
}
