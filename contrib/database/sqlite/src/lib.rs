//! `SQLite` backend for [`rskit_database`].
//!
//! This crate implements [`rskit_database`]'s contracts without adding `SQLite`
//! dependencies to the core database crate. Importing it has no side effects;
//! applications call [`register`] with an injected registry to opt in.

#![warn(missing_docs)]

mod store;

pub use store::{Config, SqliteDatabase, register};
