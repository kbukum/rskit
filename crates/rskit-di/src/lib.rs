//! Lightweight `Arc`-based runtime dependency injection container.
//!
//! # Example
//!
//! ```rust,no_run
//! use std::sync::Arc;
//! use rskit_di::Container;
//!
//! struct Config { db_url: String }
//!
//! let container = Container::new();
//! container.register(Arc::new(Config { db_url: "postgres://…".into() }));
//!
//! let cfg: Arc<Config> = container.resolve().unwrap();
//! println!("{}", cfg.db_url);
//! ```

#![warn(missing_docs)]

mod container;

pub use container::{Closeable, Container};
