//! Tenant-scoping helpers for multi-tenant database queries.
//!
//! Mirrors gokit's `database/tenant.go`.  Provides:
//!
//! - [`TenantScope`] — a builder for constructing tenant-filtered query
//!   predicate fragments.

/// Helper for building tenant-scoped SQL `WHERE` clauses.
///
/// Mirrors gokit's `ScopeToTenant` by associating a field name with a tenant
/// value. Adapter crates decide how to bind the returned value.
///
/// # Examples
///
/// ```
/// use rskit_database::TenantScope;
///
/// let scope = TenantScope::new("workspace_id", "ws-123");
/// assert_eq!(scope.where_clause(1), "workspace_id = $1");
/// assert_eq!(scope.value(), "ws-123");
/// ```
#[derive(Debug, Clone)]
pub struct TenantScope {
    column: String,
    value: String,
}

impl TenantScope {
    /// Create a new [`TenantScope`] for the given column and value.
    #[must_use]
    pub fn new(column: impl Into<String>, value: impl Into<String>) -> Self {
        Self {
            column: column.into(),
            value: value.into(),
        }
    }

    /// Return a `WHERE` clause fragment like `"workspace_id = $1"`.
    ///
    /// `param_index` is the positional parameter number for the bind
    /// placeholder (1-based for PostgreSQL `$N` syntax).
    #[must_use]
    pub fn where_clause(&self, param_index: usize) -> String {
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
    /// Returns the modified query with ` WHERE column = $N` appended, where
    /// `N` is `param_index`.
    #[must_use]
    pub fn apply(&self, query: &str, param_index: usize) -> String {
        format!("{query} WHERE {}", self.where_clause(param_index))
    }

    /// Append a tenant-scoped `AND` clause to the given SQL query.
    ///
    /// Use this when the query already has a `WHERE` clause.
    #[must_use]
    pub fn apply_and(&self, query: &str, param_index: usize) -> String {
        format!("{query} AND {}", self.where_clause(param_index))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn where_clause_basic() {
        let scope = TenantScope::new("workspace_id", "ws-123");
        assert_eq!(scope.where_clause(1), "workspace_id = $1");
    }

    #[test]
    fn where_clause_higher_index() {
        let scope = TenantScope::new("tenant_id", "t-456");
        assert_eq!(scope.where_clause(3), "tenant_id = $3");
    }

    #[test]
    fn value_accessor() {
        let scope = TenantScope::new("workspace_id", "ws-abc");
        assert_eq!(scope.value(), "ws-abc");
    }

    #[test]
    fn column_accessor() {
        let scope = TenantScope::new("org_id", "org-1");
        assert_eq!(scope.column(), "org_id");
    }

    #[test]
    fn apply_where() {
        let scope = TenantScope::new("workspace_id", "ws-1");
        let sql = scope.apply("SELECT * FROM tasks", 1);
        assert_eq!(sql, "SELECT * FROM tasks WHERE workspace_id = $1");
    }

    #[test]
    fn apply_and() {
        let scope = TenantScope::new("workspace_id", "ws-1");
        let sql = scope.apply_and("SELECT * FROM tasks WHERE status = $1", 2);
        assert_eq!(
            sql,
            "SELECT * FROM tasks WHERE status = $1 AND workspace_id = $2"
        );
    }

    #[test]
    fn empty_value() {
        let scope = TenantScope::new("workspace_id", "");
        assert_eq!(scope.value(), "");
        assert_eq!(scope.where_clause(1), "workspace_id = $1");
    }

    #[test]
    fn special_characters_in_column() {
        let scope = TenantScope::new("my_schema.workspace_id", "ws-1");
        assert_eq!(scope.where_clause(1), "my_schema.workspace_id = $1");
    }

    #[test]
    fn clone_preserves_values() {
        let scope = TenantScope::new("workspace_id", "ws-clone");
        let cloned = scope.clone();
        assert_eq!(cloned.column(), scope.column());
        assert_eq!(cloned.value(), scope.value());
    }

    #[test]
    fn debug_format() {
        let scope = TenantScope::new("workspace_id", "ws-1");
        let debug = format!("{scope:?}");
        assert!(debug.contains("workspace_id"));
        assert!(debug.contains("ws-1"));
    }
}
