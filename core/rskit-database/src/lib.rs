//! Database contracts with an in-memory default and opt-in adapter backends.
//!
//! This crate provides:
//!
//! - [`DatabaseConfig`] — backend selection and common pool configuration.
//! - [`DatabaseClient`] — vendor-neutral async execution and transaction contract.
//! - [`InMemoryDatabase`] — local backend for tests and development.
//! - [`Repository`] — a generic trait for entity CRUD operations.
//! - [`FindOpts`] — builder for paginated / filtered queries.
//! - [`query`] — HTTP query-string parsing and pagination helpers.

#![warn(missing_docs)]

/// Database configuration types.
pub mod config;
/// Database client contracts and in-memory backend.
pub mod database;
/// HTTP query parameter parsing and pagination.
pub mod query;
/// Explicit backend registration helpers.
pub mod registry;
/// Repository trait and helpers.
pub mod repository;
/// Tenant-scoping helpers for multi-tenant queries.
pub mod tenant;

pub use config::{DatabaseConfig, MemoryDatabaseConfig};
pub use database::{
    DatabaseClient, DatabaseQuery, DatabaseResult, DatabaseTransaction, InMemoryDatabase,
};
pub use query::{
    PaginatedResult, Pagination, QueryConfig, QueryParams, SortOrder, parse_query_string,
};
pub use registry::{DatabaseFactory, DatabaseRegistry, register_memory};
pub use repository::{FindOpts, Repository};
pub use tenant::TenantScope;
