//! Database contracts with feature-gated sqlx backend support.
//!
//! This crate provides:
//!
//! - [`DatabaseConfig`] — connection pool configuration with serde support.
//! - [`Database`] — an async pool wrapping `sqlx::AnyPool` when the `sqlx-any`
//!   feature is enabled.
//! - [`Repository`] — a generic trait for entity CRUD operations.
//! - [`FindOpts`] — builder for paginated / filtered queries.
//! - [`SqlRepository`] — base helper struct for SQL-backed repositories.
//! - [`query`] — HTTP query-string parsing and pagination helpers.

#![warn(missing_docs)]

/// Database configuration types.
pub mod config;
/// Database pool and `Component` implementation.
#[cfg(feature = "sqlx-any")]
pub mod database;
/// HTTP query parameter parsing and pagination.
pub mod query;
/// Explicit backend registration helpers.
pub mod registry;
/// Repository trait and helpers.
pub mod repository;
/// Tenant-scoping helpers for multi-tenant queries.
pub mod tenant;

pub use config::{DatabaseConfig, DbDriver, SslMode};
#[cfg(feature = "sqlx-any")]
pub use database::Database;
pub use query::{
    PaginatedResult, Pagination, QueryConfig, QueryParams, SortOrder, parse_query_string,
};
pub use registry::{
    DatabaseRegistry, register_mysql, register_postgres, register_sqlite, register_sqlx_any,
};
#[cfg(feature = "sqlx-any")]
pub use repository::SqlRepository;
pub use repository::{FindOpts, Repository};
pub use tenant::TenantScope;
#[cfg(feature = "sqlx-any")]
pub use tenant::set_session_variable;
