//! Git CLI implementation for command-oriented workflows.

pub mod auth;
mod manage;
mod oid;
mod read;
mod runner;
#[cfg(test)]
mod tests;
mod write;

pub(crate) use oid::parse_oid;
pub use runner::GitCli;
