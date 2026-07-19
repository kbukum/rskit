//! Tenant-scoping helpers for multi-tenant database queries.
//!
//! Mirrors gokit's `database/tenant.go`.  Provides:
//!
//! - [`TenantScope`] — a builder for constructing tenant-filtered query predicate fragments.

use std::num::NonZeroUsize;

/// Helper for building tenant-scoped SQL `WHERE` clauses.
///
/// Mirrors gokit's `ScopeToTenant` by associating a field name with a tenant value.
/// Adapter crates decide how to bind the returned value.
///
/// # Examples
///
/// ```
/// use rskit_database::TenantScope;
/// use std::num::NonZeroUsize;
///
/// let scope = TenantScope::new("workspace_id", "ws-123").unwrap();
/// assert_eq!(
///     scope.where_clause(NonZeroUsize::new(1).unwrap()),
///     "workspace_id = $1",
/// );
/// assert_eq!(scope.value(), "ws-123");
/// ```
#[derive(Debug, Clone)]
pub struct TenantScope {
    column: String,
    value: String,
}

impl TenantScope {
    /// Create a new [`TenantScope`] for the given column and value.
    ///
    /// # Errors
    /// Returns an error when the column is not a safe SQL identifier path or the tenant value is empty.
    pub fn new(
        column: impl Into<String>,
        value: impl Into<String>,
    ) -> rskit_errors::AppResult<Self> {
        let column = column.into();
        let value = value.into();
        validate_identifier_path(&column)?;
        if value.is_empty() {
            return Err(rskit_errors::AppError::invalid_input(
                "tenant.value",
                "tenant value is required",
            ));
        }
        Ok(Self { column, value })
    }

    /// Return a `WHERE` clause fragment like `"workspace_id = $1"`.
    ///
    /// `param_index` is the positional parameter number for the bind placeholder (1-based for PostgreSQL `$N` syntax).
    #[must_use]
    pub fn where_clause(&self, param_index: NonZeroUsize) -> String {
        let param_index = param_index.get();
        format!("{} = ${param_index}", self.column)
    }

    /// Return the tenant value to bind.
    #[must_use]
    pub fn value(&self) -> &str {
        &self.value
    }

    /// Return the column name.
    #[must_use]
    pub fn column(&self) -> &str {
        &self.column
    }

    /// Append a tenant-scoped `WHERE` clause to the given SQL query.
    ///
    /// Returns the modified query with ` WHERE column = $N` appended, where `N` is `param_index`.
    #[must_use]
    pub fn apply(&self, query: &str, param_index: NonZeroUsize) -> String {
        format!("{query} WHERE {}", self.where_clause(param_index))
    }

    /// Append a tenant-scoped `AND` clause to the given SQL query.
    ///
    /// Use this when the query already has a `WHERE` clause.
    #[must_use]
    pub fn apply_and(&self, query: &str, param_index: NonZeroUsize) -> String {
        format!("{query} AND {}", self.where_clause(param_index))
    }
}

pub(crate) fn validate_identifier_path(identifier: &str) -> rskit_errors::AppResult<()> {
    if identifier.is_empty() || identifier.len() > 128 {
        return Err(rskit_errors::AppError::invalid_input(
            "sql.identifier",
            "SQL identifier must be 1..=128 bytes",
        ));
    }
    if identifier.split('.').all(is_valid_identifier_segment) {
        Ok(())
    } else {
        Err(rskit_errors::AppError::invalid_input(
            "sql.identifier",
            "SQL identifier must contain only dotted ASCII identifier segments",
        ))
    }
}

fn is_valid_identifier_segment(segment: &str) -> bool {
    let mut chars = segment.chars();
    let Some(first) = chars.next() else {
        return false;
    };
    (first == '_' || first.is_ascii_alphabetic())
        && chars.all(|ch| ch == '_' || ch.is_ascii_alphanumeric())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn param(index: usize) -> NonZeroUsize {
        NonZeroUsize::new(index).unwrap()
    }

    #[test]
    fn where_clause_basic() {
        let scope = TenantScope::new("workspace_id", "ws-123").unwrap();
        assert_eq!(scope.where_clause(param(1)), "workspace_id = $1");
    }

    #[test]
    fn where_clause_higher_index() {
        let scope = TenantScope::new("tenant_id", "t-456").unwrap();
        assert_eq!(scope.where_clause(param(3)), "tenant_id = $3");
    }

    #[test]
    fn zero_parameter_index_is_not_representable() {
        assert!(NonZeroUsize::new(0).is_none());
    }

    #[test]
    fn value_accessor() {
        let scope = TenantScope::new("workspace_id", "ws-abc").unwrap();
        assert_eq!(scope.value(), "ws-abc");
    }

    #[test]
    fn column_accessor() {
        let scope = TenantScope::new("org_id", "org-1").unwrap();
        assert_eq!(scope.column(), "org_id");
    }

    #[test]
    fn apply_where() {
        let scope = TenantScope::new("workspace_id", "ws-1").unwrap();
        let sql = scope.apply("SELECT * FROM tasks", param(1));
        assert_eq!(sql, "SELECT * FROM tasks WHERE workspace_id = $1");
    }

    #[test]
    fn apply_and() {
        let scope = TenantScope::new("workspace_id", "ws-1").unwrap();
        let sql = scope.apply_and("SELECT * FROM tasks WHERE status = $1", param(2));
        assert_eq!(
            sql,
            "SELECT * FROM tasks WHERE status = $1 AND workspace_id = $2"
        );
    }

    #[test]
    fn rejects_empty_value() {
        assert!(TenantScope::new("workspace_id", "").is_err());
    }

    #[test]
    fn dotted_identifier_column() {
        let scope = TenantScope::new("my_schema.workspace_id", "ws-1").unwrap();
        assert_eq!(scope.where_clause(param(1)), "my_schema.workspace_id = $1");
    }

    #[test]
    fn rejects_unsafe_column_identifiers() {
        for column in [
            "",
            "1tenant_id",
            "tenant id",
            "tenant_id; DROP TABLE users",
            "tenant_id--",
            "\"tenant_id\"",
            ".tenant_id",
            "tenant_id.",
            "tenant..id",
        ] {
            assert!(TenantScope::new(column, "tenant-1").is_err(), "{column}");
        }
    }

    #[test]
    fn tenant_value_is_bound_data_not_identifier() {
        let scope = TenantScope::new("workspace_id", "ws-1' OR '1'='1").unwrap();
        assert_eq!(scope.where_clause(param(2)), "workspace_id = $2");
        assert_eq!(scope.value(), "ws-1' OR '1'='1");
    }

    #[test]
    fn clone_preserves_values() {
        let scope = TenantScope::new("workspace_id", "ws-clone").unwrap();
        let cloned = scope.clone();
        assert_eq!(cloned.column(), scope.column());
        assert_eq!(cloned.value(), scope.value());
    }

    #[test]
    fn debug_format() {
        let scope = TenantScope::new("workspace_id", "ws-1").unwrap();
        let debug = format!("{scope:?}");
        assert!(debug.contains("workspace_id"));
        assert!(debug.contains("ws-1"));
    }
}
