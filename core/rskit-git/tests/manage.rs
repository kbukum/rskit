//! Management behavior: refs, remotes, config, and maintenance across backends.

mod helpers;

#[path = "manage/refs_config.rs"]
mod refs_config;

#[path = "manage/cli.rs"]
mod cli;

#[path = "manage/embedded.rs"]
mod embedded;
