//! Explicit database backend registry.

use std::collections::BTreeSet;

use rskit_errors::{AppError, AppResult, ErrorCode};

use crate::DbDriver;

/// Explicit set of enabled database backends.
#[derive(Debug, Default, Clone)]
pub struct DatabaseRegistry {
    drivers: BTreeSet<DbDriver>,
}

impl DatabaseRegistry {
    /// Create an empty database backend registry.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Register a supported driver.
    #[cfg(any(feature = "postgres", feature = "mysql", feature = "sqlite"))]
    pub(crate) fn register(&mut self, driver: DbDriver) -> AppResult<()> {
        if self.drivers.contains(&driver) {
            return Err(AppError::new(
                ErrorCode::AlreadyExists,
                format!("database driver '{driver}' is already registered"),
            ));
        }
        self.drivers.insert(driver);
        Ok(())
    }

    /// Return true when the driver has been explicitly registered.
    #[must_use]
    pub fn contains(&self, driver: &DbDriver) -> bool {
        self.drivers.contains(driver)
    }

    /// Number of registered database drivers.
    #[must_use]
    pub fn len(&self) -> usize {
        self.drivers.len()
    }

    /// Return true when no drivers are registered.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.drivers.is_empty()
    }
}

/// Register the generic sqlx Any selector boundary.
#[cfg(feature = "sqlx-any")]
pub fn register_sqlx_any(registry: &mut DatabaseRegistry) -> AppResult<()> {
    #[cfg(feature = "postgres")]
    registry.register(DbDriver::Postgres)?;
    #[cfg(feature = "mysql")]
    registry.register(DbDriver::Mysql)?;
    #[cfg(feature = "sqlite")]
    registry.register(DbDriver::Sqlite)?;
    Ok(())
}

/// Register PostgreSQL backend support.
#[cfg(feature = "postgres")]
pub fn register_postgres(registry: &mut DatabaseRegistry) -> AppResult<()> {
    registry.register(DbDriver::Postgres)
}

/// Register MySQL backend support.
#[cfg(feature = "mysql")]
pub fn register_mysql(registry: &mut DatabaseRegistry) -> AppResult<()> {
    registry.register(DbDriver::Mysql)
}

/// Register SQLite backend support.
#[cfg(feature = "sqlite")]
pub fn register_sqlite(registry: &mut DatabaseRegistry) -> AppResult<()> {
    registry.register(DbDriver::Sqlite)
}

/// PostgreSQL registration is unavailable unless the `postgres` feature is enabled.
#[cfg(not(feature = "postgres"))]
pub fn register_postgres(_registry: &mut DatabaseRegistry) -> AppResult<()> {
    Err(AppError::new(
        ErrorCode::InvalidInput,
        "database postgres backend feature is not enabled",
    ))
}

/// MySQL registration is unavailable unless the `mysql` feature is enabled.
#[cfg(not(feature = "mysql"))]
pub fn register_mysql(_registry: &mut DatabaseRegistry) -> AppResult<()> {
    Err(AppError::new(
        ErrorCode::InvalidInput,
        "database mysql backend feature is not enabled",
    ))
}

/// SQLite registration is unavailable unless the `sqlite` feature is enabled.
#[cfg(not(feature = "sqlite"))]
pub fn register_sqlite(_registry: &mut DatabaseRegistry) -> AppResult<()> {
    Err(AppError::new(
        ErrorCode::InvalidInput,
        "database sqlite backend feature is not enabled",
    ))
}

/// sqlx Any registration is unavailable unless the `sqlx-any` feature is enabled.
#[cfg(not(feature = "sqlx-any"))]
pub fn register_sqlx_any(_registry: &mut DatabaseRegistry) -> AppResult<()> {
    Err(AppError::new(
        ErrorCode::InvalidInput,
        "database sqlx-any backend feature is not enabled",
    ))
}
