//! sqlx-based async database pool with repository pattern and slow-query logging.
//!
//! This crate provides:
//!
//! - [`DatabaseConfig`] — connection pool configuration with serde support.
//! - [`Database`] — an async pool wrapping [`sqlx::AnyPool`] that implements
//!   the `Component` lifecycle trait.
//! - [`Repository`] — a generic trait for entity CRUD operations.
//! - [`FindOpts`] — builder for paginated / filtered queries.
//! - [`SqlRepository`] — base helper struct for SQL-backed repositories.
//! - [`query`] — HTTP query-string parsing and pagination helpers.

#![warn(missing_docs)]

/// Database configuration types.
pub mod config;
/// Database pool and `Component` implementation.
pub mod database;
/// HTTP query parameter parsing and pagination.
pub mod query;
/// Repository trait and helpers.
pub mod repository;
/// Tenant-scoping helpers for multi-tenant queries.
pub mod tenant;

pub use config::{DatabaseConfig, DbDriver, SslMode};
pub use database::Database;
pub use query::{
    PaginatedResult, Pagination, QueryConfig, QueryParams, SortOrder, parse_query_string,
};
pub use repository::{FindOpts, Repository, SqlRepository};
pub use tenant::{TenantScope, set_session_variable};
