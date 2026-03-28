//! Repository trait and helpers for the data-access layer.

use std::marker::PhantomData;
use std::sync::Arc;

use async_trait::async_trait;

use rskit_errors::AppResult;

use crate::Database;

/// Generic repository interface for CRUD operations.
///
/// Implement this trait for each entity type.  The [`SqlRepository`] struct
/// provides a convenient base that holds a [`Database`] reference and table
/// name, but does **not** implement this trait itself because the SQL required
/// is entity-specific.
#[async_trait]
pub trait Repository<T, ID>: Send + Sync
where
    T: Send + Sync,
    ID: Send + Sync,
{
    /// Find a single entity by its primary key.
    async fn find_by_id(&self, id: &ID) -> AppResult<Option<T>>;

    /// Find all entities matching the given options.
    async fn find_all(&self, opts: FindOpts) -> AppResult<Vec<T>>;

    /// Find the first entity matching the given options.
    async fn find_first(&self, opts: FindOpts) -> AppResult<Option<T>>;

    /// Count entities matching the given options.
    async fn count(&self, opts: FindOpts) -> AppResult<i64>;

    /// Check whether an entity with the given ID exists.
    async fn exists(&self, id: &ID) -> AppResult<bool>;

    /// Insert a new entity and return the persisted version.
    async fn create(&self, entity: &T) -> AppResult<T>;

    /// Update an existing entity and return the updated version.
    async fn update(&self, entity: &T) -> AppResult<T>;

    /// Delete the entity with the given primary key.
    async fn delete(&self, id: &ID) -> AppResult<()>;

    /// Insert or update (upsert) an entity and return the result.
    async fn upsert(&self, entity: &T) -> AppResult<T>;
}

/// Options for paginated / filtered queries.
#[derive(Debug, Default)]
pub struct FindOpts {
    /// Maximum number of rows to return.
    pub limit: Option<i64>,
    /// Number of rows to skip.
    pub offset: Option<i64>,
    /// Columns to order by (e.g. `"created_at DESC"`).
    pub order_by: Vec<String>,
    /// Column-value filter pairs.
    pub filters: Vec<(String, serde_json::Value)>,
}

impl FindOpts {
    /// Set a maximum number of rows.
    #[must_use]
    pub fn with_limit(mut self, n: i64) -> Self {
        self.limit = Some(n);
        self
    }

    /// Set the row offset for pagination.
    #[must_use]
    pub fn with_offset(mut self, n: i64) -> Self {
        self.offset = Some(n);
        self
    }

    /// Append an ordering clause.
    #[must_use]
    pub fn order_by(mut self, col: &str) -> Self {
        self.order_by.push(col.to_owned());
        self
    }

    /// Append a column filter.
    #[must_use]
    pub fn filter(mut self, col: &str, val: impl Into<serde_json::Value>) -> Self {
        self.filters.push((col.to_owned(), val.into()));
        self
    }
}

/// Base helper for SQL-backed repositories.
///
/// Holds an `Arc<Database>` and a table name.  Concrete repository
/// implementations can embed this struct and delegate to its accessors when
/// building queries.
pub struct SqlRepository<T> {
    db: Arc<Database>,
    table_name: &'static str,
    _marker: PhantomData<T>,
}

impl<T> SqlRepository<T> {
    /// Create a new [`SqlRepository`] for the given table.
    pub fn new(db: Arc<Database>, table_name: &'static str) -> Self {
        Self {
            db,
            table_name,
            _marker: PhantomData,
        }
    }

    /// Return a reference to the underlying [`Database`].
    pub fn db(&self) -> &Database {
        &self.db
    }

    /// Return the table name this repository targets.
    pub fn table_name(&self) -> &str {
        self.table_name
    }
}

impl<T> std::fmt::Debug for SqlRepository<T> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("SqlRepository")
            .field("table_name", &self.table_name)
            .finish_non_exhaustive()
    }
}
