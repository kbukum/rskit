//! Structural JSON size and depth limits.

use rskit_errors::{AppError, AppResult, ErrorCode};
use serde_json::Value;

/// Limits applied before compiling or validating untrusted JSON schema values.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ValidationLimits {
    /// Maximum structural nesting depth.
    pub max_depth: usize,
    /// Maximum number of JSON nodes.
    pub max_nodes: usize,
    /// Maximum UTF-8 byte length for a single JSON string value.
    pub max_string_bytes: usize,
    /// Maximum UTF-8 byte length for a single JSON object key.
    pub max_key_bytes: usize,
    /// Maximum cumulative UTF-8 bytes across all object keys and string values.
    pub max_total_string_bytes: usize,
}

impl Default for ValidationLimits {
    fn default() -> Self {
        Self {
            max_depth: 128,
            max_nodes: 100_000,
            max_string_bytes: 1_048_576,
            max_key_bytes: 16_384,
            max_total_string_bytes: 16_777_216,
        }
    }
}

impl ValidationLimits {
    /// Create custom structural limits.
    #[must_use]
    pub const fn new(max_depth: usize, max_nodes: usize) -> Self {
        Self {
            max_depth,
            max_nodes,
            max_string_bytes: 1_048_576,
            max_key_bytes: 16_384,
            max_total_string_bytes: 16_777_216,
        }
    }

    /// Override the maximum UTF-8 byte length for a single JSON string value.
    #[must_use]
    pub const fn with_max_string_bytes(mut self, max_string_bytes: usize) -> Self {
        self.max_string_bytes = max_string_bytes;
        self
    }

    /// Override the maximum UTF-8 byte length for a single JSON object key.
    #[must_use]
    pub const fn with_max_key_bytes(mut self, max_key_bytes: usize) -> Self {
        self.max_key_bytes = max_key_bytes;
        self
    }

    /// Override the maximum cumulative UTF-8 bytes across keys and strings.
    #[must_use]
    pub const fn with_max_total_string_bytes(mut self, max_total_string_bytes: usize) -> Self {
        self.max_total_string_bytes = max_total_string_bytes;
        self
    }
}

pub(crate) fn check_json_limits(
    name: &str,
    value: &Value,
    limits: ValidationLimits,
) -> AppResult<()> {
    let mut node_count = 0usize;
    let mut total_string_bytes = 0usize;
    let mut stack = vec![(value, 1usize)];

    while let Some((node, depth)) = stack.pop() {
        node_count = node_count.saturating_add(1);
        if node_count > limits.max_nodes {
            return Err(AppError::new(
                ErrorCode::InvalidInput,
                format!(
                    "{name} exceeds maximum JSON node count {}",
                    limits.max_nodes
                ),
            ));
        }
        if depth > limits.max_depth {
            return Err(AppError::new(
                ErrorCode::InvalidInput,
                format!("{name} exceeds maximum JSON depth {}", limits.max_depth),
            ));
        }

        match node {
            Value::Array(values) => {
                stack.extend(values.iter().map(|value| (value, depth + 1)));
            }
            Value::Object(values) => {
                for key in values.keys() {
                    check_string_bytes(name, "object key", key.len(), limits)?;
                    total_string_bytes = add_string_bytes(
                        name,
                        total_string_bytes,
                        key.len(),
                        limits.max_total_string_bytes,
                    )?;
                }
                stack.extend(values.values().map(|value| (value, depth + 1)));
            }
            Value::String(value) => {
                check_string_bytes(name, "string", value.len(), limits)?;
                total_string_bytes = add_string_bytes(
                    name,
                    total_string_bytes,
                    value.len(),
                    limits.max_total_string_bytes,
                )?;
            }
            Value::Null | Value::Bool(_) | Value::Number(_) => {}
        }
    }

    Ok(())
}

fn check_string_bytes(
    name: &str,
    kind: &str,
    len: usize,
    limits: ValidationLimits,
) -> AppResult<()> {
    let max = if kind == "object key" {
        limits.max_key_bytes
    } else {
        limits.max_string_bytes
    };

    if len > max {
        return Err(AppError::new(
            ErrorCode::InvalidInput,
            format!("{name} {kind} exceeds maximum UTF-8 byte length {max}"),
        ));
    }
    Ok(())
}

fn add_string_bytes(
    name: &str,
    current: usize,
    additional: usize,
    max_total: usize,
) -> AppResult<usize> {
    let total = current.saturating_add(additional);
    if total > max_total {
        return Err(AppError::new(
            ErrorCode::InvalidInput,
            format!("{name} exceeds maximum cumulative string byte length {max_total}"),
        ));
    }
    Ok(total)
}
