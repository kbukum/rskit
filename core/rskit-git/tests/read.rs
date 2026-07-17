//! Read-flow behavior: inspection, diff, tree, log, and blame across backends.

mod helpers;

#[path = "read/inspection.rs"]
mod inspection;

#[path = "read/embedded.rs"]
mod embedded;
