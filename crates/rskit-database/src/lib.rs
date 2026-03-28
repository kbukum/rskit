//! sqlx-based async database pool with repository pattern and slow-query logging.
//!
//! This crate provides:
//!
//! - [`DatabaseConfig`] — connection pool configuration with serde support.
//! - [`Database`] — an async pool wrapping [`sqlx::AnyPool`] that implements
//!   the [`Component`](rskit_bootstrap::Component) lifecycle trait.
//! - [`Repository`] — a generic trait for entity CRUD operations.
//! - [`FindOpts`] — builder for paginated / filtered queries.
//! - [`SqlRepository`] — base helper struct for SQL-backed repositories.

#![warn(missing_docs)]

/// Database configuration types.
pub mod config;
/// Database pool and [`Component`](rskit_bootstrap::Component) implementation.
pub mod database;
/// Repository trait and helpers.
pub mod repository;

pub use config::{DatabaseConfig, DbDriver, SslMode};
pub use database::Database;
pub use repository::{FindOpts, Repository, SqlRepository};
