//! Repository trait and helpers for the data-access layer.

use async_trait::async_trait;

use rskit_errors::AppResult;

/// Generic repository interface for CRUD operations.
///
/// Implement this trait for each entity type.
/// Backend-specific repository helpers belong in adapter crates
/// so the core contract remains vendor-neutral.
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
