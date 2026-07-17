//! Write-flow behavior: staging, commits, and mutating operations across backends.

mod helpers;

#[path = "write/operations.rs"]
mod operations;

#[path = "write/cli.rs"]
mod cli;
